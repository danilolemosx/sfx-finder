//! Nome do som na pasta de destino: opcionalmente traduzido para o português e encurtado (até 3 palavras),
//! sempre seguro para nome de arquivo.

use crate::dicionario::{normalizar, Dicionario};

/// Palavras que não dizem o que é o som (somem ao encurtar).
const RUIDO: &[&str] = &[
    "and", "the", "of", "a", "an", "with", "in", "on", "into", "from", "to", "by", "at", "for", "sfx", "fx", "sound", "sounds",
    "effect", "effects", "stereo", "mono", "wav", "mp3", "aif", "aiff", "flac", "hq", "hd", "version", "ver", "take", "var",
    "variation", "e", "de", "da", "do", "com", "o", "em", "para", "por", "track", "faixa", "efeito", "efeitos", "som",
];

/// Primeira letra maiúscula ("PORTA" e "porta" → "Porta"); siglas curtas como "TV" ficam.
fn titulo(p: &str) -> String {
    if p.chars().count() <= 3 && p.chars().all(|c| c.is_uppercase() || c.is_ascii_digit()) {
        return p.to_string();
    }
    let mut cs = p.chars();
    match cs.next() {
        Some(c) => c.to_uppercase().chain(cs.flat_map(char::to_lowercase)).collect(),
        None => String::new(),
    }
}

/// Tira o que não pode em nome de arquivo (Mac e Windows).
pub fn seguro(nome: &str) -> String {
    let s: String = nome.chars().map(|c| if "/\\:*?\"<>|".contains(c) || c.is_control() { ' ' } else { c }).collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let s = s.trim_matches(|c: char| c == '.' || c == ' ').to_string();
    if s.is_empty() { "Som".into() } else { s }
}

/// Palavras de um nome de arquivo, separando "DoorSlam", "door_slam" e "Take2".
pub fn palavras_do_nome(original: &str) -> Vec<String> {
    let mut brutas: Vec<String> = Vec::new();
    for parte in original.split(|c: char| !c.is_alphanumeric()) {
        let mut atual = String::new();
        let mut ant: Option<char> = None;
        for c in parte.chars() {
            if let Some(a) = ant {
                if (a.is_lowercase() && c.is_uppercase()) || (a.is_alphabetic() && c.is_ascii_digit()) || (a.is_ascii_digit() && c.is_alphabetic()) {
                    brutas.push(std::mem::take(&mut atual));
                }
            }
            atual.push(c);
            ant = Some(c);
        }
        if !atual.is_empty() {
            brutas.push(atual);
        }
    }
    brutas
}

/// "07027096 Church Door Interior, Slams" → "Porta Igreja Interno" (traduzir + encurtar).
pub fn sugerir(original: &str, traduzir: bool, encurtar: bool, dic: &Dicionario) -> String {
    if !traduzir && !encurtar {
        return seguro(original);
    }
    let brutas = palavras_do_nome(original);
    let mut saida: Vec<String> = Vec::new();
    for p in &brutas {
        let n = normalizar(p);
        let numero = p.chars().all(|c| c.is_ascii_digit());
        if encurtar && (numero || RUIDO.contains(&n.as_str()) || p.chars().count() == 1) {
            continue;
        }
        let palavra = if traduzir && !numero { dic.traduzir_en(p).map(String::from).unwrap_or_else(|| titulo(p)) } else if numero { p.clone() } else { titulo(p) };
        // sem palavras repetidas ("Door Doors" → "Porta")
        if encurtar && saida.iter().any(|s| normalizar(s) == normalizar(&palavra)) {
            continue;
        }
        saida.push(palavra);
    }
    if encurtar {
        // conta palavras do resultado ("Chave de Fenda" vale 3)
        let mut curto: Vec<String> = Vec::new();
        let mut n = 0;
        for s in saida {
            let k = s.split_whitespace().count();
            if n > 0 && n + k > 3 {
                break;
            }
            n += k;
            curto.push(s);
            if n >= 3 {
                break;
            }
        }
        saida = curto;
    }
    if saida.is_empty() {
        return seguro(original);
    }
    seguro(&saida.join(" "))
}

#[cfg(test)]
mod testes {
    use super::*;

    fn s(o: &str, t: bool, e: bool) -> String {
        sugerir(o, t, e, Dicionario::embutido())
    }

    #[test]
    fn traduz_e_encurta() {
        assert_eq!(s("Door Metal Slam 02", true, true), "Porta Metal Batendo");
        assert_eq!(s("07027096 Church Door Interior, Slams, bolts lock door", true, true), "Igreja Porta Interno");
        assert_eq!(s("Car Door Open And Slam", true, true), "Carro Porta Abre");
        assert_eq!(s("DoorSlam_Wood", true, true), "Porta Batendo Madeira");
    }

    #[test]
    fn so_encurta() {
        assert_eq!(s("Door Metal Slam 02", false, true), "Door Metal Slam");
        assert_eq!(s("Fire engine, door slammed", false, true), "Fire Engine Door");
        assert_eq!(s("SFX - Sound Effect of a glass", false, true), "Glass");
    }

    #[test]
    fn so_traduz() {
        assert_eq!(s("Door Metal Slam 02", true, false), "Porta Metal Batendo 02");
    }

    #[test]
    fn como_esta() {
        assert_eq!(s("Door: Metal / Slam?", false, false), "Door Metal Slam");
        assert_eq!(s("...", true, true), "Som");
    }
}
