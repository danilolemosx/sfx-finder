//! Inspeção de um arquivo de áudio ou vídeo: faixas de áudio, canais, taxa, duração e timecode.

use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::Value;

use crate::ferramentas::{comando, Ferramentas};
use crate::timecode::Fps;
use crate::wav;

/// Uma faixa (stream) de áudio do arquivo.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Faixa {
    /// Índice da faixa entre as faixas de áudio (0, 1, 2…), como em `-map 0:a:N`.
    pub indice: u32,
    pub canais: u32,
    pub layout: String,
    pub sr: u32,
    pub codec: String,
}

/// Início do arquivo no timecode.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inicio {
    /// Segundos reais desde a meia-noite.
    pub segundos: f64,
    pub fps: Fps,
    /// A velocidade veio do próprio arquivo (e não do padrão escolhido na engrenagem).
    pub fps_do_arquivo: bool,
    /// "01:02:14:08".
    pub texto: String,
    /// Amostras desde a meia-noite, quando o original é um WAV com bext (valor exato).
    pub amostras: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspecao {
    pub caminho: String,
    pub nome: String,
    /// Extensão em maiúsculas ("MOV").
    pub tipo: String,
    pub video: bool,
    pub faixas: Vec<Faixa>,
    /// Total de canais (somando as faixas).
    pub canais: u32,
    /// "Mono", "Estéreo", "5.1", "Poli · 4", "8 faixas".
    pub descricao: String,
    /// Sufixo de cada canal ao separar em monos ("L", "R", "C", "LFE"… ou "A1", "A2"…).
    pub rotulos: Vec<String>,
    pub sr: u32,
    pub duracao_s: f64,
    /// Timecode do original, se houver.
    pub inicio: Option<Inicio>,
}

/// Rótulos dos canais pela ordem do ffmpeg para cada layout conhecido.
fn rotulos_layout(layout: &str) -> Option<&'static [&'static str]> {
    Some(match layout {
        "mono" => &[""],
        "stereo" => &["L", "R"],
        "2.1" => &["L", "R", "LFE"],
        "3.0" => &["L", "R", "C"],
        "quad" | "quad(side)" => &["L", "R", "Ls", "Rs"],
        "4.0" => &["L", "R", "C", "Cs"],
        "5.0" | "5.0(side)" => &["L", "R", "C", "Ls", "Rs"],
        "5.1" | "5.1(side)" => &["L", "R", "C", "LFE", "Ls", "Rs"],
        "6.1" => &["L", "R", "C", "LFE", "Cs", "Ls", "Rs"],
        "7.1" => &["L", "R", "C", "LFE", "Lsr", "Rsr", "Lss", "Rss"],
        "7.1(wide)" | "7.1(wide-side)" => &["L", "R", "C", "LFE", "Ls", "Rs", "Lc", "Rc"],
        _ => return None,
    })
}

fn descricao_layout(layout: &str, canais: u32) -> String {
    match layout {
        "mono" => "Mono".into(),
        "stereo" => "Estéreo".into(),
        l if rotulos_layout(l).is_some() => l.split('(').next().unwrap_or(l).to_string(),
        _ if canais == 1 => "Mono".into(),
        _ if canais == 2 => "Estéreo".into(),
        _ => format!("Poli · {canais}"),
    }
}

/// Deixa um nome de faixa seguro para nome de arquivo.
fn limpar(nome: &str) -> String {
    let s: String = nome.trim().chars().map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' }).collect();
    s.trim_matches('_').to_string()
}

fn num(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

/// Inspeciona `caminho`. `fps_padrao` vale quando o arquivo tem timecode mas não diz a velocidade (WAV sem iXML).
pub fn inspecionar(ferr: &Ferramentas, caminho: &Path, fps_padrao: Fps) -> Result<Inspecao> {
    let saida = comando(&ferr.ffprobe)
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(caminho)
        .output()
        .context("não foi possível rodar o ffprobe")?;
    if !saida.status.success() {
        let erro = String::from_utf8_lossy(&saida.stderr);
        bail!("arquivo não reconhecido ({})", erro.lines().last().unwrap_or("").trim());
    }
    let j: Value = serde_json::from_slice(&saida.stdout).context("resposta inesperada do ffprobe")?;
    let streams = j["streams"].as_array().cloned().unwrap_or_default();
    let formato = &j["format"];

    let mut faixas = Vec::new();
    let mut video = false;
    let mut fps_video: Option<Fps> = None;
    let mut tc_texto: Option<String> = formato["tags"]["timecode"].as_str().map(String::from);
    for s in &streams {
        let tipo = s["codec_type"].as_str().unwrap_or("");
        if tc_texto.is_none() {
            tc_texto = s["tags"]["timecode"].as_str().map(String::from);
        }
        match tipo {
            "audio" => {
                let canais = num(&s["channels"]).unwrap_or(0.0) as u32;
                if canais == 0 {
                    continue;
                }
                faixas.push(Faixa {
                    indice: faixas.len() as u32,
                    canais,
                    layout: s["channel_layout"].as_str().unwrap_or("").to_string(),
                    sr: num(&s["sample_rate"]).unwrap_or(0.0) as u32,
                    codec: s["codec_name"].as_str().unwrap_or("").to_string(),
                });
            }
            "video" => {
                // capas de MP3/M4A aparecem como vídeo com attached_pic
                if num(&s["disposition"]["attached_pic"]) == Some(1.0) {
                    continue;
                }
                video = true;
                if fps_video.is_none() {
                    fps_video = s["r_frame_rate"].as_str().and_then(|r| Fps::ler(r, false)).filter(|f| f.nominal() <= 120);
                }
            }
            _ => {}
        }
    }
    if faixas.is_empty() {
        bail!("o arquivo não tem áudio");
    }

    let canais: u32 = faixas.iter().map(|f| f.canais).sum();
    let sr = faixas.iter().map(|f| f.sr).max().unwrap_or(48_000);
    let duracao_s = num(&formato["duration"])
        .or_else(|| streams.iter().filter_map(|s| num(&s["duration"])).reduce(f64::max))
        .unwrap_or(0.0);

    // timecode: vídeo/MXF/MOV (tag "timecode") ou WAV (bext + iXML)
    let info_wav = wav::ler_info(caminho);
    let inicio = if let Some(texto) = tc_texto {
        let drop = texto.contains(';');
        let (fps, do_arquivo) = match fps_video {
            Some(f) => (Fps { drop: drop && Fps::ler(&format!("{}/{}", f.num, f.den), true).is_some_and(|x| x.drop), ..f }, true),
            None => (fps_padrao, false),
        };
        fps.tc_para_quadros(&texto).map(|q| Inicio {
            segundos: fps.quadros_para_segundos(q),
            fps,
            fps_do_arquivo: do_arquivo,
            texto: fps.quadros_para_tc(q),
            amostras: None,
        })
    } else if let Some(tr) = info_wav.time_reference.or_else(|| num(&formato["tags"]["time_reference"]).map(|v| v as u64)) {
        let ixml = info_wav.ixml_fps.as_ref().and_then(|(t, d)| Fps::ler(t, *d));
        let (fps, do_arquivo) = match ixml {
            Some(f) => (f, true),
            None => (fps_padrao, false),
        };
        let segundos = tr as f64 / sr as f64;
        let q = fps.segundos_para_quadros(segundos).floor() as u64;
        Some(Inicio { segundos, fps, fps_do_arquivo: do_arquivo, texto: fps.quadros_para_tc(q), amostras: Some(tr) })
    } else {
        None
    };

    // descrição e rótulos dos canais
    let (mut descricao, mut rotulos): (String, Vec<String>) = if faixas.len() == 1 {
        let f = &faixas[0];
        let rot = match rotulos_layout(&f.layout) {
            Some(r) if r.len() == f.canais as usize => r.iter().map(|s| s.to_string()).collect(),
            _ if f.canais == 1 => vec![String::new()],
            _ if f.canais == 2 => vec!["L".into(), "R".into()],
            _ => (1..=f.canais).map(|i| format!("A{i}")).collect(),
        };
        (descricao_layout(&f.layout, f.canais), rot)
    } else {
        let d = if faixas.iter().all(|f| f.canais == 1) {
            format!("{} faixas", faixas.len())
        } else {
            format!("{} faixas · {canais} canais", faixas.len())
        };
        (d, (1..=canais).map(|i| format!("A{i}")).collect())
    };
    // gravadores de som: nome das faixas no iXML (BOOM, LAV1…)
    if info_wav.nomes_faixas.len() == canais as usize && canais > 1 {
        let nomes: Vec<String> = info_wav.nomes_faixas.iter().map(|n| limpar(n)).collect();
        let unicos = nomes.iter().all(|n| !n.is_empty()) && nomes.iter().collect::<std::collections::HashSet<_>>().len() == nomes.len();
        if unicos {
            // arquivo de gravador de som: cada canal é um microfone, não um layout de mixagem
            rotulos = nomes;
            if faixas.len() == 1 {
                descricao = format!("Poli · {canais}");
            }
        }
    }

    let nome = caminho.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tipo = caminho.extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default();
    Ok(Inspecao {
        caminho: caminho.to_string_lossy().into_owned(),
        nome,
        tipo,
        video,
        faixas,
        canais,
        descricao,
        rotulos,
        sr,
        duracao_s,
        inicio,
    })
}
