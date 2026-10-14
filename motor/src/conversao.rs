//! Conversão: monta o comando do ffmpeg (e do raw2bmx no MXF), acompanha o progresso e grava as saídas.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::ferramentas::{comando, Ferramentas};
use crate::formatos::Formato;
use crate::inspecao::Inspecao;
use crate::timecode::Fps;

/// Escolhas da tela.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Opcoes {
    pub formato: Formato,
    /// Taxa de saída; `None` mantém a do original.
    pub taxa: Option<u32>,
    /// 16, 24 ou 32 (float). Ignorado nos formatos com perda.
    pub bits: u8,
    /// Qualidade dos formatos com perda; `None` usa o padrão do formato.
    pub kbps: Option<u32>,
    /// Separar cada canal num arquivo mono.
    pub separar: bool,
    /// Pasta de saída; `None` salva ao lado do original.
    pub destino: Option<PathBuf>,
    /// Velocidade do timecode quando o arquivo não informa (usada no MXF).
    pub fps: Fps,
    /// Nome do arquivo de saída (sem extensão); `None` usa o nome do original.
    #[serde(default)]
    pub nome: Option<String>,
    /// Só um pedaço do som: início e fim em segundos.
    #[serde(default)]
    pub trecho: Option<[f64; 2]>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resultado {
    pub arquivos: Vec<String>,
    pub avisos: Vec<String>,
}

/// Quantos arquivos a conversão vai gerar.
pub fn n_saidas(ins: &Inspecao, op: &Opcoes) -> usize {
    if op.separar || op.formato == Formato::Mxf { ins.canais as usize } else { 1 }
}

fn khz(sr: u32) -> String {
    let v = sr as f64 / 1000.0;
    if v.fract() == 0.0 { format!("{v:.0} kHz") } else { format!("{v:.1} kHz").replace('.', ",") }
}

/// Primeiro caminho livre: "nome.ext", "nome_2.ext", "nome_3.ext"…
pub fn livre(pasta: &Path, base: &str, ext: &str, reservados: &[PathBuf]) -> PathBuf {
    let ocupado = |p: &Path| p.exists() || reservados.iter().any(|r| r == p);
    let p = pasta.join(format!("{base}.{ext}"));
    if !ocupado(&p) {
        return p;
    }
    (2..).map(|n| pasta.join(format!("{base}_{n}.{ext}"))).find(|p| !ocupado(p)).unwrap()
}

/// Layout para o downmix quando o formato aceita menos canais.
fn layout_downmix(max: u32) -> &'static str {
    match max {
        1 => "mono",
        2 => "stereo",
        6 => "5.1",
        _ => "7.1",
    }
}

/// Uma saída do ffmpeg: os canais que entram nela e o arquivo.
struct Saida {
    filtro: String,
    canais: u32,
    caminho: PathBuf,
    /// Sufixo do canal (vazio quando não separa).
    rotulo: String,
}

pub fn converter(
    ferr: &Ferramentas,
    ins: &Inspecao,
    op: &Opcoes,
    progresso: &mut dyn FnMut(f64),
    cancelar: &AtomicBool,
) -> Result<Resultado> {
    let info = op.formato.info();
    let mxf = op.formato == Formato::Mxf;
    let mut res = Resultado::default();

    let entrada = Path::new(&ins.caminho);
    let pasta = match &op.destino {
        Some(d) => d.clone(),
        None => entrada.parent().map(Path::to_path_buf).unwrap_or_default(),
    };
    std::fs::create_dir_all(&pasta).with_context(|| format!("não foi possível criar a pasta {}", pasta.display()))?;
    let base = match &op.nome {
        Some(n) if !n.trim().is_empty() => n.trim().to_string(),
        _ => entrada.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "audio".into()),
    };
    // trecho: o som começa no ponto escolhido e perde o timecode do original
    let trecho = op.trecho.map(|[a, b]| (a.max(0.0), (b.min(ins.duracao_s) - a.max(0.0)).max(0.01)));
    let ins_t;
    let ins = match trecho {
        Some((_, d)) => {
            ins_t = Inspecao { duracao_s: d, inicio: None, ..ins.clone() };
            &ins_t
        }
        None => ins,
    };

    // taxa e bits finais
    let pedida = op.taxa.unwrap_or(ins.sr);
    let sr = info.taxa_final(pedida);
    if sr != pedida {
        res.avisos.push(format!("{} usa {}: {} virou {}", info.nome, khz(sr), khz(pedida), khz(sr)));
    }
    let bits = info.bits_final(op.bits);
    if let Some(b) = bits {
        if b != op.bits && !info.com_perda {
            res.avisos.push(format!("{} grava até {b} bits", info.nome));
        }
    }

    // entrada: uma faixa, ou várias faixas juntadas num arquivo só
    let mut grafo = String::new();
    let fonte = if ins.faixas.len() == 1 {
        "[0:a:0]".to_string()
    } else {
        for f in &ins.faixas {
            grafo.push_str(&format!("[0:a:{}]aresample={}[in{}];", f.indice, ins.sr, f.indice));
        }
        let ent: String = ins.faixas.iter().map(|f| format!("[in{}]", f.indice)).collect();
        grafo.push_str(&format!("{ent}amerge=inputs={}[mix];", ins.faixas.len()));
        "[mix]".to_string()
    };

    // alinhamento ao quadro (MXF): silêncio antes até o quadro anterior e último quadro completo
    let fps = ins.inicio.as_ref().map(|i| i.fps).unwrap_or(op.fps);
    let seg_inicio = ins.inicio.as_ref().map(|i| i.segundos).unwrap_or(0.0);
    let por_quadro = sr as f64 * fps.den as f64 / fps.num as f64;
    let qf = fps.segundos_para_quadros(seg_inicio);
    let q0 = (qf + 1e-6).floor();
    let pad_ini = if mxf { ((qf - q0).max(0.0) * por_quadro).round() as u64 } else { 0 };
    let dur_amostras = (ins.duracao_s * sr as f64).ceil() as u64;
    let quadros = ((pad_ini + dur_amostras) as f64 / por_quadro - 1e-9).ceil().max(1.0);
    let total_mxf = (quadros * por_quadro).ceil() as u64;

    let mut fim = format!("aresample={sr}");
    if bits == Some(16) {
        fim.push_str(":dither_method=triangular");
    }
    if mxf {
        if pad_ini > 0 {
            fim.push_str(&format!(",adelay=delays={pad_ini}S:all=1"));
        }
        fim.push_str(&format!(",apad=whole_len={total_mxf}"));
    }

    // pasta temporária do MXF (ao lado do destino, para não lotar o disco do sistema)
    let tmp = if mxf {
        if ferr.raw2bmx.is_none() {
            bail!("raw2bmx não encontrado: não dá para gravar MXF");
        }
        let t = pasta.join(format!(".sfxfinder-tmp-{}-{}", std::process::id(), seg_inicio.to_bits() ^ dur_amostras));
        std::fs::create_dir_all(&t)?;
        Some(t)
    } else {
        None
    };
    let pasta_ff = tmp.clone().unwrap_or_else(|| pasta.clone());
    let ext_ff = if mxf { "wav" } else { info.extensao };

    // saídas
    let mut saidas: Vec<Saida> = Vec::new();
    let mut reservados: Vec<PathBuf> = Vec::new();
    let separar = op.separar && ins.canais > 1;
    if separar {
        let n = ins.canais as usize;
        let alvos: String = (0..n).map(|i| format!("[s{i}]")).collect();
        grafo.push_str(&format!("{fonte}asplit={n}{alvos};"));
        for i in 0..n {
            let rot = ins.rotulos.get(i).cloned().filter(|r| !r.is_empty()).unwrap_or_else(|| format!("A{}", i + 1));
            let caminho = livre(&pasta_ff, &format!("{base}_{rot}"), ext_ff, &reservados);
            reservados.push(caminho.clone());
            grafo.push_str(&format!("[s{i}]pan=mono|c0=c{i},{fim}[o{i}];"));
            saidas.push(Saida { filtro: format!("[o{i}]"), canais: 1, caminho, rotulo: rot });
        }
    } else {
        let mut canais = ins.canais;
        let mut cadeia = fim.clone();
        if canais > info.max_canais {
            let lay = layout_downmix(info.max_canais);
            cadeia = format!("aresample=ochl={lay},{fim}");
            res.avisos.push(format!("{} aceita até {} canais: {} virou {}", info.nome, info.max_canais, ins.descricao, if lay == "stereo" { "estéreo" } else { lay }));
            canais = info.max_canais;
        }
        grafo.push_str(&format!("{fonte}{cadeia}[o0];"));
        let caminho = livre(&pasta_ff, &base, ext_ff, &reservados);
        saidas.push(Saida { filtro: "[o0]".into(), canais, caminho, rotulo: String::new() });
    }
    grafo.pop(); // último ";"

    // comando do ffmpeg
    let mut cmd = comando(&ferr.ffmpeg);
    cmd.args(["-hide_banner", "-nostdin", "-y", "-v", "error", "-nostats", "-progress", "pipe:1"]);
    if let Some((a, d)) = trecho {
        cmd.args(["-ss", &format!("{a:.6}"), "-t", &format!("{d:.6}")]);
    }
    // sem os metadados do original (metadados herdados já derrubaram o Avid)
    cmd.arg("-i").arg(entrada).args(["-filter_complex", &grafo, "-map_metadata", "-1"]);
    let time_reference = ins.inicio.as_ref().map(|i| match i.amostras {
        Some(a) if sr == ins.sr => a,
        _ => (i.segundos * sr as f64).round() as u64,
    });
    for s in &saidas {
        cmd.args(["-map", &s.filtro]);
        if mxf {
            cmd.args(Formato::Wav.info().args_codec(bits, None, s.canais, &ferr.encoders));
        } else {
            cmd.args(info.args_codec(bits, op.kbps, s.canais, &ferr.encoders));
        }
        cmd.args(info.args_conteiner());
        if op.formato == Formato::Wav {
            cmd.args(["-write_bext", "1", "-metadata", &format!("time_reference={}", time_reference.unwrap_or(0))]);
        }
        cmd.arg(&s.caminho);
    }

    let escala = if mxf { 0.85 } else { 1.0 };
    let r = rodar(cmd, ins.duracao_s, &mut |p| progresso(p * escala), cancelar);
    if let Err(e) = r {
        for s in &saidas {
            let _ = std::fs::remove_file(&s.caminho);
        }
        if let Some(t) = &tmp {
            let _ = std::fs::remove_dir_all(t);
        }
        return Err(e);
    }

    if let Some(t) = &tmp {
        let r = gravar_mxf(ferr, ins, &saidas, &pasta, &base, fps, q0 as u64, t, &mut res, cancelar);
        let _ = std::fs::remove_dir_all(t);
        r?;
        if pad_ini > 0 {
            res.avisos.push("início alinhado ao quadro (silêncio antes do primeiro quadro)".into());
        }
    } else {
        res.arquivos = saidas.iter().map(|s| s.caminho.to_string_lossy().into_owned()).collect();
    }
    progresso(1.0);
    Ok(res)
}

/// Converte os WAVs temporários em MXF OP-Atom. Separado: um clipe por canal. Juntos: um clipe com N faixas.
#[allow(clippy::too_many_arguments)]
fn gravar_mxf(
    ferr: &Ferramentas,
    ins: &Inspecao,
    saidas: &[Saida],
    pasta: &Path,
    base: &str,
    fps: Fps,
    quadro_inicio: u64,
    tmp: &Path,
    res: &mut Resultado,
    cancelar: &AtomicBool,
) -> Result<()> {
    let raw2bmx = ferr.raw2bmx.as_ref().ok_or_else(|| anyhow!("raw2bmx não encontrado"))?;
    let tc = fps.quadros_para_tc(quadro_inicio);
    let taxa = format!("{}/{}", fps.num, fps.den);
    let mut reservados: Vec<PathBuf> = Vec::new();
    for (k, s) in saidas.iter().enumerate() {
        if cancelar.load(Ordering::Relaxed) {
            bail!("cancelado");
        }
        let clipe = if s.rotulo.is_empty() { base.to_string() } else { format!("{base}_{}", s.rotulo) };
        let prefixo = tmp.join(format!("mxf{k}"));
        let saida = comando(raw2bmx)
            .args(["-t", "avid", "-f", &taxa, "-y", &tc, "--clip", &clipe, "--tape", base, "-o"])
            .arg(&prefixo)
            .arg("--wave")
            .arg(&s.caminho)
            .output()
            .context("não foi possível rodar o raw2bmx")?;
        if !saida.status.success() {
            bail!("erro ao gravar o MXF: {}", ultima_linha(&saida.stderr).or_else(|| ultima_linha(&saida.stdout)).unwrap_or_default());
        }
        // raw2bmx grava <prefixo>_a1.mxf, _a2.mxf… (uma faixa por arquivo)
        for c in 0..s.canais as usize {
            let gerado = PathBuf::from(format!("{}_a{}.mxf", prefixo.display(), c + 1));
            if !gerado.is_file() {
                bail!("o raw2bmx não gerou {}", gerado.display());
            }
            let nome = if !s.rotulo.is_empty() {
                format!("{base}_{}", s.rotulo)
            } else if s.canais > 1 {
                format!("{base}_{}", ins.rotulos.get(c).filter(|r| !r.is_empty()).cloned().unwrap_or_else(|| format!("A{}", c + 1)))
            } else {
                base.to_string()
            };
            let destino = livre(pasta, &nome, "mxf", &reservados);
            std::fs::rename(&gerado, &destino).with_context(|| format!("não foi possível gravar {}", destino.display()))?;
            reservados.push(destino.clone());
            res.arquivos.push(destino.to_string_lossy().into_owned());
        }
    }
    Ok(())
}

fn ultima_linha(b: &[u8]) -> Option<String> {
    String::from_utf8_lossy(b).lines().map(str::trim).filter(|l| !l.is_empty()).last().map(String::from)
}

/// Roda o ffmpeg lendo o progresso (`-progress pipe:1`). Mata o processo se `cancelar` virar verdadeiro.
fn rodar(mut cmd: std::process::Command, duracao_s: f64, progresso: &mut dyn FnMut(f64), cancelar: &AtomicBool) -> Result<()> {
    let mut filho = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().context("não foi possível rodar o ffmpeg")?;
    let mut erro_pipe = filho.stderr.take().unwrap();
    let leitor_erro = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = erro_pipe.read_to_end(&mut s);
        s
    });
    let saida = filho.stdout.take().unwrap();
    for linha in BufReader::new(saida).lines() {
        let Ok(linha) = linha else { break };
        if cancelar.load(Ordering::Relaxed) {
            let _ = filho.kill();
            let _ = filho.wait();
            bail!("cancelado");
        }
        if let Some(v) = linha.strip_prefix("out_time_us=").or_else(|| linha.strip_prefix("out_time_ms=")) {
            if let (Ok(us), true) = (v.trim().parse::<f64>(), duracao_s > 0.0) {
                progresso((us / 1e6 / duracao_s).clamp(0.0, 1.0));
            }
        }
    }
    let status = filho.wait()?;
    let erro = leitor_erro.join().unwrap_or_default();
    if cancelar.load(Ordering::Relaxed) {
        bail!("cancelado");
    }
    if !status.success() {
        bail!("erro do ffmpeg: {}", ultima_linha(&erro).unwrap_or_else(|| format!("código {status}")));
    }
    Ok(())
}
