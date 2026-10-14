//! Dicionário embutido: português → inglês (para buscar "porta batendo" e achar "door slam") e
//! inglês → português (para renomear os sons). Fica em `dicionario/*.txt`, fácil de ampliar.

use std::collections::HashMap;
use std::sync::OnceLock;

pub struct Dicionario {
    pt_en: HashMap<String, Vec<String>>,
    en_pt: HashMap<String, String>,
    /// Maior número de palavras numa expressão em português ("chave de fenda" = 3).
    max_palavras: usize,
}

/// Minúsculas e sem acentos ("Portão" → "portao").
pub fn normalizar(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' | 'å' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            c => c,
        })
        .collect()
}

/// Palavras de um texto (letras e números), já normalizadas.
pub fn palavras(s: &str) -> Vec<String> {
    normalizar(s).split(|c: char| !c.is_alphanumeric()).filter(|p| !p.is_empty()).map(String::from).collect()
}

fn ler(texto: &str) -> impl Iterator<Item = (String, Vec<String>)> + '_ {
    texto.lines().filter_map(|l| {
        let l = l.trim();
        if l.is_empty() || l.starts_with('#') {
            return None;
        }
        let (k, v) = l.split_once(':')?;
        let valores = v.split(',').map(|x| x.trim().trim_matches('"').trim().to_string()).filter(|x| !x.is_empty()).collect();
        Some((k.trim().to_string(), valores))
    })
}

impl Dicionario {
    pub fn embutido() -> &'static Dicionario {
        static D: OnceLock<Dicionario> = OnceLock::new();
        D.get_or_init(|| Dicionario::novo(include_str!("dicionario/pt-en.txt"), include_str!("dicionario/en-pt.txt")))
    }

    pub fn novo(pt_en: &str, en_pt: &str) -> Dicionario {
        let mut d = Dicionario { pt_en: HashMap::new(), en_pt: HashMap::new(), max_palavras: 1 };
        for (k, v) in ler(pt_en) {
            let chave = palavras(&k).join(" ");
            d.max_palavras = d.max_palavras.max(chave.split(' ').count());
            let lista = d.pt_en.entry(chave).or_default();
            for x in v {
                let x = x.to_lowercase();
                if !lista.contains(&x) {
                    lista.push(x);
                }
            }
        }
        for (k, v) in ler(en_pt) {
            if let Some(t) = v.into_iter().next() {
                d.en_pt.entry(normalizar(&k)).or_insert(t);
            }
        }
        d
    }

    /// Formas-base possíveis de uma palavra em português (plural, feminino, gerúndio, particípio, verbo).
    fn formas_pt(p: &str) -> Vec<String> {
        let mut v = vec![p.to_string()];
        let troca = |v: &mut Vec<String>, fim: &str, novos: &[&str]| {
            if let Some(raiz) = p.strip_suffix(fim) {
                if raiz.len() >= 2 {
                    v.extend(novos.iter().map(|n| format!("{raiz}{n}")));
                }
            }
        };
        troca(&mut v, "oes", &["ao"]);
        troca(&mut v, "aes", &["ao"]);
        troca(&mut v, "aos", &["ao"]);
        troca(&mut v, "ais", &["al"]);
        troca(&mut v, "eis", &["el"]);
        troca(&mut v, "res", &["r"]);
        troca(&mut v, "zes", &["z"]);
        troca(&mut v, "ses", &["s"]);
        troca(&mut v, "ns", &["m"]);
        troca(&mut v, "inhos", &["o", "a"]);
        troca(&mut v, "inhas", &["a", "o"]);
        troca(&mut v, "inho", &["o"]);
        troca(&mut v, "inha", &["a"]);
        troca(&mut v, "ando", &["ar"]);
        troca(&mut v, "endo", &["er", "ir"]);
        troca(&mut v, "indo", &["ir"]);
        troca(&mut v, "ados", &["ar", "ado"]);
        troca(&mut v, "adas", &["ar", "ado", "ada"]);
        troca(&mut v, "ado", &["ar"]);
        troca(&mut v, "ada", &["ar", "ado"]);
        troca(&mut v, "idos", &["er", "ir", "ido"]);
        troca(&mut v, "idas", &["er", "ir", "ido"]);
        troca(&mut v, "ido", &["er", "ir"]);
        troca(&mut v, "ida", &["er", "ir", "ido"]);
        troca(&mut v, "ou", &["ar"]);
        troca(&mut v, "eu", &["er"]);
        troca(&mut v, "iu", &["ir"]);
        troca(&mut v, "am", &["ar"]);
        troca(&mut v, "em", &["er", "ir"]);
        troca(&mut v, "s", &[""]);
        troca(&mut v, "a", &["o", "ar"]);
        troca(&mut v, "as", &["o", "a"]);
        troca(&mut v, "e", &["er", "ir", "ar"]);
        troca(&mut v, "o", &["ar", "er", "ir"]);
        v
    }

    fn achar_pt(&self, chave: &str) -> Option<&Vec<String>> {
        if let Some(t) = self.pt_en.get(chave) {
            return Some(t);
        }
        // numa expressão, só a última palavra varia ("chaves de fenda" também acha pela primeira)
        let mut partes: Vec<String> = chave.split(' ').map(String::from).collect();
        let n = partes.len();
        for i in [n - 1, 0] {
            let original = partes[i].clone();
            for f in Self::formas_pt(&original).into_iter().skip(1) {
                partes[i] = f;
                if let Some(t) = self.pt_en.get(&partes.join(" ")) {
                    return Some(t);
                }
            }
            partes[i] = original;
            if n == 1 {
                break;
            }
        }
        None
    }

    /// Divide a busca em grupos: cada palavra (ou expressão conhecida, como "ar condicionado") com as
    /// traduções em inglês. Ex.: "porta batendo" → [("porta", [door, gate]), ("batendo", [hit, slam…])].
    pub fn expandir(&self, palavras_busca: &[String]) -> Vec<(String, Vec<String>)> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < palavras_busca.len() {
            let mut achou = false;
            for n in (1..=self.max_palavras.min(palavras_busca.len() - i)).rev() {
                let chave = palavras_busca[i..i + n].join(" ");
                if let Some(t) = self.achar_pt(&chave) {
                    let t: Vec<String> = t.iter().filter(|x| **x != chave).cloned().collect();
                    out.push((chave, t));
                    i += n;
                    achou = true;
                    break;
                }
            }
            if !achou {
                out.push((palavras_busca[i].clone(), Vec::new()));
                i += 1;
            }
        }
        out
    }

    /// Formas-base possíveis de uma palavra em inglês (doors, slamming, closed, flies).
    fn formas_en(p: &str) -> Vec<String> {
        let mut v = vec![p.to_string()];
        let tira_dobro = |r: &str| -> Option<String> {
            let b = r.as_bytes();
            (b.len() >= 3 && b[b.len() - 1] == b[b.len() - 2]).then(|| r[..r.len() - 1].to_string())
        };
        if let Some(r) = p.strip_suffix("ies") {
            v.push(format!("{r}y"));
        }
        if let Some(r) = p.strip_suffix("es") {
            v.push(r.to_string());
        }
        if let Some(r) = p.strip_suffix('s') {
            v.push(r.to_string());
        }
        for fim in ["ing", "ed", "er"] {
            if let Some(r) = p.strip_suffix(fim) {
                if r.len() >= 2 {
                    v.extend(tira_dobro(r));
                    v.push(r.to_string());
                    v.push(format!("{r}e"));
                }
            }
        }
        v
    }

    /// Tradução de uma palavra em inglês para o português, se houver.
    pub fn traduzir_en(&self, palavra: &str) -> Option<&str> {
        let p = normalizar(palavra);
        Self::formas_en(&p).iter().find_map(|f| self.en_pt.get(f)).map(String::as_str)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn exp(q: &str) -> Vec<(String, Vec<String>)> {
        Dicionario::embutido().expandir(&palavras(q))
    }

    #[test]
    fn busca_em_portugues() {
        let e = exp("Porta batendo");
        assert_eq!(e.len(), 2);
        assert!(e[0].1.contains(&"door".to_string()));
        assert!(e[1].1.contains(&"slam".to_string()));
        assert!(exp("portões")[0].1.contains(&"gate".to_string()));
        assert!(exp("passarinhos")[0].1.contains(&"bird".to_string()));
        assert!(exp("latidos")[0].1.contains(&"bark".to_string()));
        assert!(exp("fechou")[0].1.contains(&"close".to_string()));
        assert!(exp("explosões")[0].1.contains(&"explosion".to_string()));
    }

    #[test]
    fn expressoes() {
        let e = exp("chave de fenda velha");
        assert_eq!(e[0].0, "chave de fenda");
        assert_eq!(e[0].1, ["screwdriver"]);
        assert!(e[1].1.contains(&"old".to_string()));
    }

    #[test]
    fn palavra_desconhecida_fica() {
        let e = exp("xyzw door");
        assert!(e[0].1.is_empty());
        assert_eq!(e[1].0, "door");
    }

    #[test]
    fn ingles_para_portugues() {
        let d = Dicionario::embutido();
        assert_eq!(d.traduzir_en("Doors"), Some("Porta"));
        assert_eq!(d.traduzir_en("slamming"), Some("Batendo"));
        assert_eq!(d.traduzir_en("closed"), Some("Fecha"));
        assert_eq!(d.traduzir_en("flies"), Some("Moscas"));
        assert_eq!(d.traduzir_en("qwerty"), None);
    }
}
