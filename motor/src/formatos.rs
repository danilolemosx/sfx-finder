//! Formatos de saída e o que cada um aceita (taxas, bits, canais, qualidade).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Formato {
    Wav,
    Mxf,
    Aiff,
    Flac,
    Alac,
    Mp3,
    Aac,
    Ac3,
    Eac3,
    Ogg,
    Opus,
    Wma,
    Mp2,
    Caf,
    W64,
}

/// Grupo do formato na interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Grupo {
    Edicao,
    Principal,
    Outros,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoFormato {
    pub id: Formato,
    /// Nome no botão ("MXF AVID").
    pub nome: &'static str,
    /// Descrição curta ao lado do nome grande.
    pub descricao: &'static str,
    pub extensao: &'static str,
    pub grupo: Grupo,
    /// Formato com perda (MP3, AAC…): usa kbps em vez de bits.
    pub com_perda: bool,
    /// Bits aceitos (vazio = não se aplica).
    pub bits: &'static [u8],
    /// Taxas aceitas (vazio = qualquer uma).
    pub taxas: &'static [u32],
    /// Máximo de canais num arquivo (acima disso, "manter juntos" faz downmix).
    pub max_canais: u32,
    /// Opções de qualidade em kbps (por par estéreo nos codecs marcados em `kbps_por_par`).
    pub kbps: &'static [u32],
    pub kbps_padrao: u32,
    pub kbps_por_par: bool,
    /// Grava o timecode do original.
    pub timecode: bool,
}

const PCM_BITS: &[u8] = &[16, 24, 32];
const BITS_INT: &[u8] = &[16, 24];
const TAXAS_DOLBY: &[u32] = &[32_000, 44_100, 48_000];

pub const FORMATOS: &[InfoFormato] = &[
    InfoFormato { id: Formato::Wav, nome: "WAV", descricao: "BROADCAST WAVE\nTIMECODE DO ORIGINAL", extensao: "wav", grupo: Grupo::Edicao, com_perda: false, bits: PCM_BITS, taxas: &[], max_canais: 64, kbps: &[], kbps_padrao: 0, kbps_por_par: false, timecode: true },
    InfoFormato { id: Formato::Mxf, nome: "MXF AVID", descricao: "AVID OP-ATOM\nTIMECODE DO ORIGINAL", extensao: "mxf", grupo: Grupo::Edicao, com_perda: false, bits: BITS_INT, taxas: &[48_000], max_canais: 64, kbps: &[], kbps_padrao: 0, kbps_por_par: false, timecode: true },
    InfoFormato { id: Formato::Aiff, nome: "AIFF", descricao: "PCM DA APPLE\nSEM TIMECODE", extensao: "aif", grupo: Grupo::Edicao, com_perda: false, bits: BITS_INT, taxas: &[], max_canais: 64, kbps: &[], kbps_padrao: 0, kbps_por_par: false, timecode: false },
    InfoFormato { id: Formato::Flac, nome: "FLAC", descricao: "SEM PERDA\nCOMPACTADO", extensao: "flac", grupo: Grupo::Principal, com_perda: false, bits: BITS_INT, taxas: &[], max_canais: 8, kbps: &[], kbps_padrao: 0, kbps_por_par: false, timecode: false },
    InfoFormato { id: Formato::Alac, nome: "ALAC", descricao: "APPLE LOSSLESS\n.M4A SEM PERDA", extensao: "m4a", grupo: Grupo::Principal, com_perda: false, bits: BITS_INT, taxas: &[], max_canais: 8, kbps: &[], kbps_padrao: 0, kbps_por_par: false, timecode: false },
    InfoFormato { id: Formato::Mp3, nome: "MP3", descricao: "COMPACTADO\nTOCA EM TUDO", extensao: "mp3", grupo: Grupo::Principal, com_perda: true, bits: &[], taxas: &[32_000, 44_100, 48_000], max_canais: 2, kbps: &[128, 192, 256, 320], kbps_padrao: 320, kbps_por_par: false, timecode: false },
    InfoFormato { id: Formato::Aac, nome: "AAC", descricao: "COMPACTADO\n.M4A", extensao: "m4a", grupo: Grupo::Principal, com_perda: true, bits: &[], taxas: &[8_000, 11_025, 12_000, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000, 64_000, 88_200, 96_000], max_canais: 8, kbps: &[128, 192, 256, 320], kbps_padrao: 256, kbps_por_par: true, timecode: false },
    InfoFormato { id: Formato::Ac3, nome: "AC3", descricao: "DOLBY DIGITAL\nATÉ 5.1", extensao: "ac3", grupo: Grupo::Principal, com_perda: true, bits: &[], taxas: TAXAS_DOLBY, max_canais: 6, kbps: &[192, 384, 448, 640], kbps_padrao: 448, kbps_por_par: false, timecode: false },
    InfoFormato { id: Formato::Eac3, nome: "E-AC3", descricao: "DOLBY DIGITAL PLUS\nATÉ 7.1", extensao: "eac3", grupo: Grupo::Outros, com_perda: true, bits: &[], taxas: TAXAS_DOLBY, max_canais: 8, kbps: &[384, 640, 1024], kbps_padrao: 640, kbps_por_par: false, timecode: false },
    InfoFormato { id: Formato::Ogg, nome: "OGG", descricao: "VORBIS\nCOMPACTADO", extensao: "ogg", grupo: Grupo::Outros, com_perda: true, bits: &[], taxas: &[], max_canais: 8, kbps: &[128, 192, 256, 320], kbps_padrao: 192, kbps_por_par: true, timecode: false },
    InfoFormato { id: Formato::Opus, nome: "OPUS", descricao: "COMPACTADO\nSEMPRE 48 KHZ", extensao: "opus", grupo: Grupo::Outros, com_perda: true, bits: &[], taxas: &[48_000], max_canais: 8, kbps: &[96, 128, 192, 256], kbps_padrao: 128, kbps_por_par: true, timecode: false },
    InfoFormato { id: Formato::Wma, nome: "WMA", descricao: "WINDOWS MEDIA\nCOMPACTADO", extensao: "wma", grupo: Grupo::Outros, com_perda: true, bits: &[], taxas: &[8_000, 11_025, 16_000, 22_050, 32_000, 44_100, 48_000], max_canais: 2, kbps: &[128, 192], kbps_padrao: 192, kbps_por_par: false, timecode: false },
    InfoFormato { id: Formato::Mp2, nome: "MP2", descricao: "MPEG LAYER II\nBROADCAST", extensao: "mp2", grupo: Grupo::Outros, com_perda: true, bits: &[], taxas: TAXAS_DOLBY, max_canais: 2, kbps: &[192, 256, 384], kbps_padrao: 256, kbps_por_par: false, timecode: false },
    InfoFormato { id: Formato::Caf, nome: "CAF", descricao: "CORE AUDIO\nPCM", extensao: "caf", grupo: Grupo::Outros, com_perda: false, bits: PCM_BITS, taxas: &[], max_canais: 64, kbps: &[], kbps_padrao: 0, kbps_por_par: false, timecode: false },
    InfoFormato { id: Formato::W64, nome: "W64", descricao: "WAVE 64\nARQUIVOS GIGANTES", extensao: "w64", grupo: Grupo::Outros, com_perda: false, bits: PCM_BITS, taxas: &[], max_canais: 64, kbps: &[], kbps_padrao: 0, kbps_por_par: false, timecode: false },
];

impl Formato {
    pub fn info(self) -> &'static InfoFormato {
        FORMATOS.iter().find(|f| f.id == self).expect("formato sem descrição")
    }
}

impl InfoFormato {
    /// Taxa final: a pedida (ou a original), trocada pela aceita mais próxima se o formato limitar.
    pub fn taxa_final(&self, pedida: u32) -> u32 {
        if self.taxas.is_empty() || self.taxas.contains(&pedida) {
            return pedida;
        }
        // a menor taxa aceita que não perca agudos; se não houver, a maior
        self.taxas.iter().copied().find(|&t| t >= pedida).unwrap_or(*self.taxas.last().unwrap())
    }

    /// Bits finais: o pedido, ou o maior aceito abaixo dele.
    pub fn bits_final(&self, pedido: u8) -> Option<u8> {
        if self.bits.is_empty() {
            return None;
        }
        Some(self.bits.iter().copied().filter(|&b| b <= pedido).max().unwrap_or(self.bits[0]))
    }

    /// Taxa de bits total (bit/s) para `canais` canais.
    pub fn bitrate(&self, kbps: Option<u32>, canais: u32) -> u32 {
        let k = kbps.filter(|k| *k > 0).unwrap_or(self.kbps_padrao);
        let pares = if self.kbps_por_par { canais.div_ceil(2).max(1) } else { 1 };
        k * pares * 1000
    }

    /// Argumentos do codificador no ffmpeg (depois de `-map`).
    pub fn args_codec(&self, bits: Option<u8>, kbps: Option<u32>, canais: u32, encoders: &Encoders) -> Vec<String> {
        let b = bits.unwrap_or(24);
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let br = self.bitrate(kbps, canais).to_string();
        match self.id {
            Formato::Wav | Formato::Mxf | Formato::W64 => s(&["-c:a", match b { 16 => "pcm_s16le", 32 => "pcm_f32le", _ => "pcm_s24le" }]),
            Formato::Caf => s(&["-c:a", match b { 16 => "pcm_s16be", 32 => "pcm_f32be", _ => "pcm_s24be" }]),
            Formato::Aiff => s(&["-c:a", if b == 16 { "pcm_s16be" } else { "pcm_s24be" }]),
            Formato::Flac => {
                if b == 16 { s(&["-c:a", "flac", "-sample_fmt", "s16"]) } else { s(&["-c:a", "flac", "-sample_fmt", "s32", "-bits_per_raw_sample", "24"]) }
            }
            Formato::Alac => {
                if b == 16 { s(&["-c:a", "alac", "-sample_fmt", "s16p"]) } else { s(&["-c:a", "alac", "-sample_fmt", "s32p", "-bits_per_raw_sample", "24"]) }
            }
            Formato::Mp3 => vec!["-c:a".into(), "libmp3lame".into(), "-b:a".into(), br],
            Formato::Aac => vec!["-c:a".into(), "aac".into(), "-b:a".into(), br],
            Formato::Ac3 => vec!["-c:a".into(), "ac3".into(), "-b:a".into(), br],
            Formato::Eac3 => vec!["-c:a".into(), "eac3".into(), "-b:a".into(), br],
            Formato::Ogg => {
                if encoders.libvorbis {
                    vec!["-c:a".into(), "libvorbis".into(), "-b:a".into(), br]
                } else {
                    vec!["-c:a".into(), "vorbis".into(), "-strict".into(), "-2".into(), "-b:a".into(), br]
                }
            }
            Formato::Opus => {
                let mut v = if encoders.libopus {
                    vec!["-c:a".into(), "libopus".into(), "-b:a".into(), br]
                } else {
                    vec!["-c:a".into(), "opus".into(), "-strict".into(), "-2".into(), "-b:a".into(), br]
                };
                if encoders.libopus && canais > 2 {
                    v.extend(["-mapping_family".into(), "1".into()]);
                }
                v
            }
            Formato::Wma => vec!["-c:a".into(), "wmav2".into(), "-b:a".into(), br],
            Formato::Mp2 => vec!["-c:a".into(), "mp2".into(), "-b:a".into(), br],
        }
    }

    /// Argumentos do contêiner no ffmpeg (antes do nome do arquivo).
    pub fn args_conteiner(&self) -> Vec<String> {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        match self.id {
            Formato::Wav | Formato::Mxf => s(&["-f", "wav", "-rf64", "auto"]),
            Formato::Aiff => s(&["-f", "aiff"]),
            Formato::Alac | Formato::Aac => s(&["-f", "ipod"]),
            Formato::Caf => s(&["-f", "caf"]),
            Formato::W64 => s(&["-f", "w64"]),
            Formato::Ogg => s(&["-f", "ogg"]),
            Formato::Opus => s(&["-f", "opus"]),
            Formato::Wma => s(&["-f", "asf"]),
            Formato::Mp3 => s(&["-f", "mp3", "-id3v2_version", "3"]),
            Formato::Mp2 => s(&["-f", "mp2"]),
            Formato::Ac3 => s(&["-f", "ac3"]),
            Formato::Eac3 => s(&["-f", "eac3"]),
            Formato::Flac => s(&["-f", "flac"]),
        }
    }
}

/// Codificadores opcionais disponíveis no ffmpeg embutido.
#[derive(Debug, Clone, Copy, Default)]
pub struct Encoders {
    pub libvorbis: bool,
    pub libopus: bool,
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn limites() {
        let mp3 = Formato::Mp3.info();
        assert_eq!(mp3.taxa_final(96_000), 48_000);
        assert_eq!(mp3.taxa_final(22_050), 32_000);
        assert_eq!(Formato::Mxf.info().taxa_final(44_100), 48_000);
        assert_eq!(Formato::Wav.info().taxa_final(96_000), 96_000);
        assert_eq!(Formato::Flac.info().bits_final(32), Some(24));
        assert_eq!(Formato::Mp3.info().bits_final(24), None);
        assert_eq!(Formato::Aac.info().bitrate(Some(256), 6), 768_000);
        assert_eq!(Formato::Ac3.info().bitrate(None, 6), 448_000);
        // cada formato tem descrição
        for f in FORMATOS {
            assert_eq!(f.id.info().nome, f.nome);
        }
    }
}
