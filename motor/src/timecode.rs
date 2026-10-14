//! Timecode: velocidade (fps), conversão entre texto, quadros e segundos (com drop frame).

use serde::{Deserialize, Serialize};

/// Velocidade do timecode, como fração (ex.: 24000/1001), e se é drop frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fps {
    pub num: u32,
    pub den: u32,
    #[serde(default)]
    pub drop: bool,
}

impl Fps {
    pub const fn new(num: u32, den: u32, drop: bool) -> Self {
        Fps { num, den, drop }
    }

    /// Quadros por segundo "redondos" usados na contagem do timecode (24, 25, 30, 60…).
    pub fn nominal(&self) -> u64 {
        ((self.num as f64 / self.den as f64).round() as u64).max(1)
    }

    /// Quadros descartados por minuto no drop frame (2 em 29,97; 4 em 59,94).
    fn descarte(&self) -> u64 {
        if self.drop { self.nominal() / 15 } else { 0 }
    }

    /// Texto para mostrar: "23,976", "25", "29,97 DF".
    pub fn rotulo(&self) -> String {
        let v = self.num as f64 / self.den as f64;
        let mut s = if self.den == 1 { format!("{}", self.num) } else { format!("{:.3}", v).trim_end_matches('0').to_string() };
        if s.ends_with('.') {
            s.pop();
        }
        s = s.replace('.', ",");
        if self.drop { format!("{s} DF") } else { s }
    }

    /// Lê "24000/1001", "25/1", "25", "29.97", "23,976".
    pub fn ler(texto: &str, drop: bool) -> Option<Fps> {
        let t = texto.trim().replace(',', ".");
        let (num, den) = if let Some((a, b)) = t.split_once('/') {
            (a.trim().parse::<u32>().ok()?, b.trim().parse::<u32>().ok()?)
        } else {
            let v: f64 = t.parse().ok()?;
            let inteiro = v.round();
            if (v - inteiro).abs() < 1e-6 {
                (inteiro as u32, 1)
            } else {
                // 23,976 / 29,97 / 59,94 / 47,952 → N*1000/1001
                ((inteiro * 1000.0) as u32, 1001)
            }
        };
        if num == 0 || den == 0 {
            return None;
        }
        let f = Fps { num, den, drop: false };
        let pode_drop = den == 1001 && f.nominal() % 30 == 0;
        Some(Fps { drop: drop && pode_drop, ..f })
    }

    /// Converte quadros (contagem do timecode) em segundos reais desde a meia-noite.
    pub fn quadros_para_segundos(&self, quadros: u64) -> f64 {
        quadros as f64 * self.den as f64 / self.num as f64
    }

    /// Converte segundos reais em quadros (fracionário).
    pub fn segundos_para_quadros(&self, s: f64) -> f64 {
        s * self.num as f64 / self.den as f64
    }

    /// "hh:mm:ss:ff" (ou ";ff" em drop frame) → número do quadro.
    pub fn tc_para_quadros(&self, tc: &str) -> Option<u64> {
        let partes: Vec<&str> = tc.trim().split(|c| c == ':' || c == ';' || c == '.' || c == ',').collect();
        if partes.len() != 4 {
            return None;
        }
        let v: Vec<u64> = partes.iter().map(|p| p.parse::<u64>().ok()).collect::<Option<_>>()?;
        let (h, m, s, f) = (v[0], v[1], v[2], v[3]);
        let n = self.nominal();
        if m > 59 || s > 59 || f >= n {
            return None;
        }
        let minutos = 60 * h + m;
        let bruto = (h * 3600 + m * 60 + s) * n + f;
        Some(bruto - self.descarte() * (minutos - minutos / 10))
    }

    /// Número do quadro → "hh:mm:ss:ff" (";" antes dos quadros em drop frame).
    pub fn quadros_para_tc(&self, quadros: u64) -> String {
        let n = self.nominal();
        let d = self.descarte();
        let mut q = quadros % (24 * 3600 * n);
        if d > 0 {
            let por_10min = n * 600 - d * 9;
            let por_min = n * 60 - d;
            let dez = q / por_10min;
            let resto = q % por_10min;
            q += d * 9 * dez;
            if resto > d {
                q += d * ((resto - d) / por_min);
            }
        }
        let f = q % n;
        let s = (q / n) % 60;
        let m = (q / (n * 60)) % 60;
        let h = q / (n * 3600);
        let sep = if self.drop { ';' } else { ':' };
        format!("{h:02}:{m:02}:{s:02}{sep}{f:02}")
    }
}

/// Velocidades oferecidas na interface.
pub const VELOCIDADES: [Fps; 7] = [
    Fps::new(24000, 1001, false),
    Fps::new(24, 1, false),
    Fps::new(25, 1, false),
    Fps::new(30000, 1001, false),
    Fps::new(30000, 1001, true),
    Fps::new(30, 1, false),
    Fps::new(60000, 1001, false),
];

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ndf_ida_e_volta() {
        let f = Fps::new(24000, 1001, false);
        let q = f.tc_para_quadros("01:02:14:08").unwrap();
        assert_eq!(q, (3600 + 2 * 60 + 14) * 24 + 8);
        assert_eq!(f.quadros_para_tc(q), "01:02:14:08");
        // 23,976 a 48 kHz = 2002 amostras por quadro
        assert_eq!((f.quadros_para_segundos(q) * 48_000.0).round() as u64, q * 2002);
    }

    #[test]
    fn drop_frame() {
        let f = Fps::new(30000, 1001, true);
        // 00:01:00;02 é o primeiro quadro depois do salto
        assert_eq!(f.tc_para_quadros("00:01:00;02"), Some(1800));
        assert_eq!(f.quadros_para_tc(1800), "00:01:00;02");
        assert_eq!(f.tc_para_quadros("00:10:00;00"), Some(17982));
        for q in [0u64, 1799, 1800, 17981, 17982, 107892, 2_589_407] {
            assert_eq!(f.tc_para_quadros(&f.quadros_para_tc(q)), Some(q), "quadro {q}");
        }
    }

    #[test]
    fn rotulos() {
        assert_eq!(Fps::new(24000, 1001, false).rotulo(), "23,976");
        assert_eq!(Fps::new(25, 1, false).rotulo(), "25");
        assert_eq!(Fps::new(30000, 1001, true).rotulo(), "29,97 DF");
        assert_eq!(Fps::ler("23.976", false), Some(Fps::new(24000, 1001, false)));
        assert_eq!(Fps::ler("25/1", true), Some(Fps::new(25, 1, false)));
    }
}
