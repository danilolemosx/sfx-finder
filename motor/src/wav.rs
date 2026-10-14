//! Leitura dos blocos de um WAV que o ffprobe não entrega por inteiro: o timecode do `bext`
//! (time_reference) e o `iXML` dos gravadores de som (velocidade do timecode e nome das faixas).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct InfoWav {
    /// Amostras desde a meia-noite (bloco bext).
    pub time_reference: Option<u64>,
    /// Velocidade do timecode no iXML (ex.: "24000/1001") e se é drop frame.
    pub ixml_fps: Option<(String, bool)>,
    /// Nome de cada canal no iXML (pela ordem dos canais), quando houver.
    pub nomes_faixas: Vec<String>,
    /// Descrição do bloco bext (as bibliotecas profissionais escrevem o que é o som).
    pub descricao: String,
}

/// Lê bext e iXML de um WAV/RF64/BW64. Devolve vazio se não for WAV.
pub fn ler_info(caminho: &Path) -> InfoWav {
    ler(caminho).unwrap_or_default()
}

fn ler(caminho: &Path) -> std::io::Result<InfoWav> {
    let mut f = File::open(caminho)?;
    let mut cab = [0u8; 12];
    f.read_exact(&mut cab)?;
    if !matches!(&cab[0..4], b"RIFF" | b"RF64" | b"BW64") || &cab[8..12] != b"WAVE" {
        return Ok(InfoWav::default());
    }
    let tamanho_total = f.metadata()?.len();
    let mut info = InfoWav::default();
    let mut pos = 12u64;
    while pos + 8 <= tamanho_total {
        f.seek(SeekFrom::Start(pos))?;
        let mut c = [0u8; 8];
        f.read_exact(&mut c)?;
        let id = &c[0..4];
        let tam = u32::from_le_bytes([c[4], c[5], c[6], c[7]]) as u64;
        if id == b"data" && tam == 0xFFFF_FFFF {
            break; // RF64: o tamanho real fica no ds64; os blocos que interessam vêm antes
        }
        if id == b"bext" && tam >= 346 {
            let mut d = vec![0u8; 346];
            f.read_exact(&mut d)?;
            let lo = u32::from_le_bytes([d[338], d[339], d[340], d[341]]) as u64;
            let hi = u32::from_le_bytes([d[342], d[343], d[344], d[345]]) as u64;
            info.time_reference = Some(hi << 32 | lo);
            let fim = d[..256].iter().position(|&b| b == 0).unwrap_or(256);
            info.descricao = String::from_utf8_lossy(&d[..fim]).trim().to_string();
        } else if id == b"iXML" && tam < 4 << 20 {
            let mut d = vec![0u8; tam as usize];
            f.read_exact(&mut d)?;
            ler_ixml(&String::from_utf8_lossy(&d), &mut info);
        }
        pos += 8 + tam + (tam & 1);
    }
    Ok(info)
}

fn tag<'a>(xml: &'a str, nome: &str) -> Option<&'a str> {
    let ini = format!("<{nome}>");
    let fim = format!("</{nome}>");
    let a = xml.find(&ini)? + ini.len();
    let b = xml[a..].find(&fim)? + a;
    Some(xml[a..b].trim())
}

fn ler_ixml(xml: &str, info: &mut InfoWav) {
    if let Some(taxa) = tag(xml, "TIMECODE_RATE") {
        let drop = tag(xml, "TIMECODE_FLAG").is_some_and(|f| f.eq_ignore_ascii_case("DF"));
        info.ixml_fps = Some((taxa.to_string(), drop));
    }
    if let Some(lista) = tag(xml, "TRACK_LIST") {
        let mut faixas: Vec<(usize, String)> = Vec::new();
        for bloco in lista.split("<TRACK>").skip(1) {
            let idx = tag(bloco, "INTERLEAVE_INDEX").or_else(|| tag(bloco, "CHANNEL_INDEX")).and_then(|v| v.parse::<usize>().ok());
            if let (Some(i), Some(nome)) = (idx, tag(bloco, "NAME")) {
                faixas.push((i, nome.to_string()));
            }
        }
        faixas.sort_by_key(|f| f.0);
        let completo = faixas.iter().enumerate().all(|(k, f)| f.0 == k + 1);
        if completo && faixas.iter().any(|f| !f.1.is_empty()) {
            info.nomes_faixas = faixas.into_iter().map(|f| f.1).collect();
        }
    }
}

/// Monta um bloco iXML mínimo (usado nos testes para imitar um gravador de som).
pub fn ixml_exemplo(fps: &str, drop: bool, nomes: &[&str]) -> String {
    let faixas: String = nomes
        .iter()
        .enumerate()
        .map(|(i, n)| format!("<TRACK><CHANNEL_INDEX>{0}</CHANNEL_INDEX><INTERLEAVE_INDEX>{0}</INTERLEAVE_INDEX><NAME>{n}</NAME></TRACK>", i + 1))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><BWFXML><SPEED><TIMECODE_RATE>{fps}</TIMECODE_RATE><TIMECODE_FLAG>{}</TIMECODE_FLAG></SPEED><TRACK_LIST><TRACK_COUNT>{}</TRACK_COUNT>{faixas}</TRACK_LIST></BWFXML>",
        if drop { "DF" } else { "NDF" },
        nomes.len()
    )
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_ixml() {
        let mut i = InfoWav::default();
        ler_ixml(&ixml_exemplo("24000/1001", false, &["BOOM", "LAV 1", "LAV 2"]), &mut i);
        assert_eq!(i.ixml_fps, Some(("24000/1001".into(), false)));
        assert_eq!(i.nomes_faixas, vec!["BOOM", "LAV 1", "LAV 2"]);
    }
}
