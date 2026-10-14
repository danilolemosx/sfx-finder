//! A cesta: copiar ou converter os sons escolhidos para a pasta temática, com o nome novo,
//! e anotar a origem de cada um no ORIGEM.txt. Também a forma de onda e a prévia para ouvir.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::AtomicBool;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::conversao::{converter, livre, Opcoes};
use crate::ferramentas::{comando, Ferramentas};
use crate::formatos::Formato;
use crate::inspecao::inspecionar;
use crate::nomes::seguro;
use crate::timecode::Fps;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemCesta {
    pub caminho: String,
    /// Nome final, sem extensão.
    pub nome: String,
    /// Só um pedaço do som (início e fim em segundos).
    #[serde(default)]
    pub trecho: Option<[f64; 2]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Acao {
    Copiar,
    Converter,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envio {
    /// Pasta final (destino + subpasta temática).
    pub pasta: PathBuf,
    pub acao: Acao,
    pub formato: Formato,
    pub taxa: Option<u32>,
    pub bits: u8,
    pub kbps: Option<u32>,
    pub fps: Fps,
    /// Escrever o ORIGEM.txt.
    pub origem: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Enviado {
    pub caminho: String,
    pub arquivos: Vec<String>,
    pub avisos: Vec<String>,
    pub erro: Option<String>,
}

fn seg(v: f64) -> String {
    format!("{v:.2}").replace('.', ",")
}

/// Copia ou converte um som. Devolve os arquivos gravados.
pub fn enviar(ferr: &Ferramentas, item: &ItemCesta, envio: &Envio, progresso: &mut dyn FnMut(f64), cancelar: &AtomicBool) -> Result<Enviado> {
    let entrada = Path::new(&item.caminho);
    if !entrada.is_file() {
        bail!("o arquivo não existe mais: {}", item.caminho);
    }
    std::fs::create_dir_all(&envio.pasta).with_context(|| format!("não foi possível criar a pasta {}", envio.pasta.display()))?;
    let nome = seguro(&item.nome);
    let mut res = Enviado { caminho: item.caminho.clone(), ..Default::default() };
    match envio.acao {
        Acao::Copiar => {
            let ext = entrada.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_else(|| "wav".into());
            let destino = livre(&envio.pasta, &nome, &ext, &[]);
            match item.trecho {
                None => {
                    std::fs::copy(entrada, &destino).with_context(|| format!("não foi possível copiar para {}", destino.display()))?;
                }
                Some([a, b]) => {
                    // só o pedaço, no mesmo formato: PCM/FLAC regravados no mesmo codec (corte na amostra exata);
                    // MP3 e outros com perda copiados sem reconverter (corte no quadro mais próximo)
                    let codec = codec_de(ferr, entrada);
                    let mesmo = match codec.as_deref() {
                        Some(c) if c.starts_with("pcm_") || c == "flac" => c.to_string(),
                        _ => "copy".to_string(),
                    };
                    let saida = comando(&ferr.ffmpeg)
                        .args(["-hide_banner", "-nostdin", "-v", "error", "-ss", &format!("{a:.6}"), "-t", &format!("{:.6}", (b - a).max(0.01)), "-i"])
                        .arg(entrada)
                        .args(["-map", "0:a:0", "-c:a", &mesmo, "-map_metadata", "-1"])
                        .args(if ext.eq_ignore_ascii_case("bwf") { &["-f", "wav"][..] } else { &[][..] })
                        .arg(&destino)
                        .output()
                        .context("não foi possível rodar o ffmpeg")?;
                    if !saida.status.success() {
                        let _ = std::fs::remove_file(&destino);
                        bail!("erro ao cortar o trecho: {}", String::from_utf8_lossy(&saida.stderr).lines().last().unwrap_or_default());
                    }
                }
            }
            progresso(1.0);
            res.arquivos.push(destino.to_string_lossy().into_owned());
        }
        Acao::Converter => {
            let ins = inspecionar(ferr, entrada, envio.fps)?;
            let op = Opcoes {
                formato: envio.formato,
                taxa: envio.taxa,
                bits: envio.bits,
                kbps: envio.kbps,
                separar: false,
                destino: Some(envio.pasta.clone()),
                fps: envio.fps,
                nome: Some(nome),
                trecho: item.trecho,
            };
            let r = converter(ferr, &ins, &op, progresso, cancelar)?;
            res.arquivos = r.arquivos;
            res.avisos = r.avisos;
        }
    }
    if envio.origem {
        let trecho = item.trecho.map(|[a, b]| format!("   [trecho {} a {} s]", seg(a), seg(b))).unwrap_or_default();
        let nomes: Vec<String> =
            res.arquivos.iter().filter_map(|a| Path::new(a).file_name().map(|n| n.to_string_lossy().into_owned())).collect();
        anotar_origem(&envio.pasta, &format!("{}  ←  {}{trecho}", nomes.join(" + "), item.caminho))?;
    }
    Ok(res)
}

/// Codec do primeiro áudio do arquivo ("pcm_s24le", "flac", "mp3"…).
fn codec_de(ferr: &Ferramentas, arq: &Path) -> Option<String> {
    let s = comando(&ferr.ffprobe)
        .args(["-v", "error", "-select_streams", "a:0", "-show_entries", "stream=codec_name", "-of", "csv=p=0"])
        .arg(arq)
        .output()
        .ok()?;
    let c = String::from_utf8_lossy(&s.stdout).trim().to_string();
    (!c.is_empty()).then_some(c)
}

/// Acrescenta uma linha no ORIGEM.txt da pasta.
pub fn anotar_origem(pasta: &Path, linha: &str) -> Result<()> {
    let arq = pasta.join("ORIGEM.txt");
    let novo = !arq.exists();
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&arq).with_context(|| format!("não foi possível gravar {}", arq.display()))?;
    if novo {
        writeln!(f, "De onde veio cada som (SFX Finder)\r\n")?;
    }
    write!(f, "{linha}\r\n")?;
    Ok(())
}

/// Subpastas temáticas de uma pasta de destino (sem as ocultas), em ordem alfabética.
pub fn subpastas(pasta: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(pasta)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| !n.starts_with('.') && !n.starts_with(".sfxfinder-tmp"))
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_key(|n| n.to_lowercase());
    v
}

/// Forma de onda: `n` picos de 0 a 255 (o maior vira 255). Lê no máximo os primeiros 15 minutos.
pub fn picos(ferr: &Ferramentas, caminho: &Path, n: usize) -> Result<Vec<u8>> {
    let mut filho = comando(&ferr.ffmpeg)
        .args(["-hide_banner", "-nostdin", "-v", "error", "-t", "900", "-i"])
        .arg(caminho)
        .args(["-map", "0:a:0", "-ac", "1", "-ar", "2000", "-c:a", "pcm_s16le", "-f", "wav", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("não foi possível rodar o ffmpeg")?;
    let mut bruto = Vec::new();
    filho.stdout.take().unwrap().read_to_end(&mut bruto)?;
    filho.wait()?;
    // WAV pelo cano: as amostras começam depois do bloco "data" (o ffmpeg do app não tem a saída crua s16le)
    let inicio = bruto.windows(4).take(4096).position(|w| w == b"data").map(|p| p + 8).unwrap_or(bruto.len());
    let amostras: Vec<f32> = bruto[inicio..].chunks_exact(2).map(|b| (i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).abs()).collect();
    if amostras.is_empty() {
        bail!("não foi possível ler o áudio");
    }
    let mut v = vec![0f32; n];
    for (i, a) in amostras.iter().enumerate() {
        let k = i * n / amostras.len();
        if *a > v[k] {
            v[k] = *a;
        }
    }
    let max = v.iter().cloned().fold(0.0001f32, f32::max);
    Ok(v.iter().map(|x| ((x / max).sqrt() * 255.0).round() as u8).collect())
}

/// Prévia em WAV para formatos que o player da janela não toca (AIFF no Windows, WMA…).
/// Fica em `cache` com um nome derivado do caminho; reaproveitada nas próximas vezes.
pub fn previa(ferr: &Ferramentas, caminho: &Path, cache: &Path) -> Result<PathBuf> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    caminho.hash(&mut h);
    std::fs::metadata(caminho).ok().and_then(|m| m.modified().ok()).hash(&mut h);
    std::fs::create_dir_all(cache)?;
    let saida = cache.join(format!("{:016x}.wav", h.finish()));
    if saida.is_file() {
        return Ok(saida);
    }
    let tmp = cache.join(format!("{:016x}.tmp.wav", h.finish()));
    let r = comando(&ferr.ffmpeg)
        .args(["-hide_banner", "-nostdin", "-y", "-v", "error", "-i"])
        .arg(caminho)
        .args(["-map", "0:a:0", "-map_metadata", "-1", "-c:a", "pcm_s16le"])
        .arg(&tmp)
        .output()
        .context("não foi possível rodar o ffmpeg")?;
    if !r.status.success() {
        let _ = std::fs::remove_file(&tmp);
        bail!("não foi possível preparar a prévia");
    }
    std::fs::rename(&tmp, &saida)?;
    Ok(saida)
}

/// Velocidade padrão do MXF de efeitos (25 quadros).
pub fn fps_padrao() -> Fps {
    Fps::new(25, 1, false)
}
