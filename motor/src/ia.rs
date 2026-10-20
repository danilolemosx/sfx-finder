//! IA local (Qwen3.5-2B rodando no llama.cpp, sem internet): transforma a busca em termos de biblioteca
//! de SFX em inglês ("portão velho rangendo" → "gate creak", "rusty hinge"…) e dá nomes naturais em
//! português aos sons ("Gunshot_Pistol_9mm_Muffled" → "Tiro de Pistola Abafado").
//! O llama-server roda como um programa à parte, só na máquina (127.0.0.1), enquanto o app está aberto.
//! As pistas do dicionário entram junto com cada pedido: com elas o modelo pequeno não erra o sentido
//! das palavras ("gaveta" é drawer, não wallet).

use std::path::Path;
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::dicionario::{palavras, Dicionario};
use crate::ferramentas::comando;

pub struct Ia {
    filho: Child,
    porta: u16,
    agente: ureq::Agent,
}

impl Drop for Ia {
    fn drop(&mut self) {
        let _ = self.filho.kill();
        let _ = self.filho.wait();
    }
}

/// O que a IA entendeu de uma busca.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Termos {
    /// Tradução fiel da busca.
    pub ingles: String,
    /// Termos curtos como os das bibliotecas (o primeiro é o mais literal).
    pub termos: Vec<String>,
}

const SIS_BUSCA: &str = "You turn an audio editor's search (in Brazilian Portuguese or English) into English search terms for a sound effects library.
Library file names are in English, like \"Door Wood Slam Hard\", \"Gate Metal Creak Open\", \"Gunshot Pistol Distant\", \"Crowd Cheer Stadium\".
Answer JSON: \"english\" = faithful English translation of the search; \"terms\" = 4 to 8 short English terms (1 to 3 words) that such file names would contain: the literal translation first, then sound-effect synonyms. Every term MUST be English.
When dictionary hints are given, trust them for the meaning of each word.";

const EX_BUSCA: &[(&str, &str)] = &[
    ("porta batendo\n(dictionary hints: porta = door, gate; batendo = slam, knock, hit)", r#"{"english":"door slamming","terms":["door slam","door bang","door knock","door close hard","door shut"]}"#),
    ("som de tensão para cena de terror\n(dictionary hints: som = sound; tensao = tension, suspense, drone; terror = horror, scary, creepy)", r#"{"english":"tension sound for a horror scene","terms":["horror tension","suspense drone","eerie ambience","horror stinger","creepy atmosphere","dark drone"]}"#),
    ("criança chorando\n(dictionary hints: crianca = child, children, kid; chorando = cry, crying, sob)", r#"{"english":"child crying","terms":["child crying","kid cry","baby crying","child sob","toddler cry"]}"#),
];

const SIS_NOME: &str = "You rename sound effect files for a Brazilian film editor. Given ONE English file name (and sometimes its description), write a natural Brazilian Portuguese name saying what the sound is.
Rules: Title Case; no numbers, codes, library names or extensions; main noun first, then what happens (e.g. \"Porta Batendo\", \"Portão de Ferro Abrindo\", \"Tiro de Pistola Abafado\", \"Multidão Comemorando\"). Use verbs in the gerund (Batendo, Abrindo, Rangendo) for actions. Ignore words that are not about the sound (Sound Effect, SFX, numbers, take, version).
Dictionary hints, when given, show the right meaning of each word.";

const EX_NOME: &[(&str, &str)] = &[
    ("Door Metal Slam 02\n(hints: door = Porta; metal = Metal; slam = Batendo)", "Porta de Metal Batendo"),
    ("07027096 Church Door Interior, Slams, bolts lock door\n(hints: church = Igreja; door = Porta; slam = Batendo; lock = Fechadura)", "Porta de Igreja Batendo"),
    ("CROWD_Stadium_Cheer_Goal_LRG\n(hints: crowd = Multidão; stadium = Estádio; cheer = Torcida)", "Torcida Comemorando Gol"),
];

/// Porta livre na máquina para o servidor.
fn porta_livre() -> Result<u16> {
    let l = std::net::TcpListener::bind("127.0.0.1:0")?;
    Ok(l.local_addr()?.port())
}

impl Ia {
    /// Abre o llama-server com o modelo e espera ele ficar pronto (alguns segundos).
    pub fn iniciar(servidor: &Path, modelo: &Path) -> Result<Ia> {
        if !modelo.is_file() {
            bail!("modelo de IA não encontrado: {}", modelo.display());
        }
        let porta = porta_livre()?;
        let mut cmd = comando(servidor);
        cmd.arg("-m").arg(modelo);
        cmd.args(["--host", "127.0.0.1", "--port", &porta.to_string(), "-c", "4096", "-np", "1", "-ngl", "99"]);
        cmd.args(["--reasoning-budget", "0", "--no-webui", "--log-disable"]);
        let filho = cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().context("não foi possível abrir a IA (llama-server)")?;
        let agente = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(2)).timeout(Duration::from_secs(60)).build();
        let mut ia = Ia { filho, porta, agente };
        let t0 = Instant::now();
        loop {
            if let Ok(Some(st)) = ia.filho.try_wait() {
                bail!("a IA fechou ao abrir ({st})");
            }
            let ok = ia.agente.get(&format!("http://127.0.0.1:{porta}/health")).timeout(Duration::from_secs(2)).call().is_ok();
            if ok {
                return Ok(ia);
            }
            if t0.elapsed() > Duration::from_secs(120) {
                bail!("a IA demorou demais para abrir");
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    /// Número do processo (para fechar um servidor que tenha ficado aberto se o app cair).
    pub fn pid(&self) -> u32 {
        self.filho.id()
    }

    /// Uma pergunta com exemplos; a resposta segue o `esquema` (JSON garantido pelo llama.cpp).
    fn perguntar(&self, sistema: &str, exemplos: &[(String, String)], pergunta: &str, esquema: Value, max_tokens: u32) -> Result<Value> {
        let mut msgs = vec![json!({"role": "system", "content": sistema})];
        for (p, r) in exemplos {
            msgs.push(json!({"role": "user", "content": p}));
            msgs.push(json!({"role": "assistant", "content": r}));
        }
        msgs.push(json!({"role": "user", "content": pergunta}));
        let corpo = json!({
            "messages": msgs,
            "temperature": 0.1,
            "max_tokens": max_tokens,
            "response_format": {"type": "json_schema", "json_schema": {"schema": esquema}},
            "chat_template_kwargs": {"enable_thinking": false},
        });
        let r: Value = self
            .agente
            .post(&format!("http://127.0.0.1:{}/v1/chat/completions", self.porta))
            .send_json(corpo)
            .context("a IA não respondeu")?
            .into_json()?;
        let texto = r["choices"][0]["message"]["content"].as_str().unwrap_or_default();
        serde_json::from_str(texto).with_context(|| format!("resposta estranha da IA: {texto}"))
    }

    /// Termos em inglês para a busca.
    pub fn termos_busca(&self, busca: &str, dic: &Dicionario) -> Result<Termos> {
        let pistas = pistas_pt(busca, dic);
        let pergunta = if pistas.is_empty() { busca.to_string() } else { format!("{busca}\n(dictionary hints: {})", pistas.join("; ")) };
        let ex: Vec<(String, String)> = EX_BUSCA.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        let esquema = json!({"type": "object", "properties": {"english": {"type": "string"}, "terms": {"type": "array", "items": {"type": "string"}, "minItems": 3, "maxItems": 8}}, "required": ["english", "terms"]});
        let v = self.perguntar(SIS_BUSCA, &ex, &pergunta, esquema, 200)?;
        let mut termos: Vec<String> = Vec::new();
        for t in v["terms"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            let t = t.trim().to_lowercase();
            if !t.is_empty() && t.split_whitespace().count() <= 4 && !termos.contains(&t) {
                termos.push(t);
            }
        }
        Ok(Termos { ingles: v["english"].as_str().unwrap_or_default().trim().to_lowercase(), termos })
    }

    /// Nome em português para um som. `encurtar`: até 3 palavras principais.
    pub fn nome(&self, original: &str, descricao: &str, encurtar: bool, dic: &Dicionario) -> Result<String> {
        let pistas = pistas_en(&format!("{original} {descricao}"), dic);
        let mut pergunta = original.to_string();
        if !descricao.is_empty() && descricao != original {
            pergunta.push_str(&format!("\n(description: {descricao})"));
        }
        if !pistas.is_empty() {
            pergunta.push_str(&format!("\n(hints: {})", pistas.join("; ")));
        }
        let limite = if encurtar { "Use 1 to 3 main words (small words like \"de\", \"no\", \"com\" do not count)." } else { "Translate the whole meaningful name, without shortening it." };
        let sistema = format!("{SIS_NOME}\n{limite}");
        let ex: Vec<(String, String)> = EX_NOME.iter().map(|(a, b)| (a.to_string(), json!({"name": b}).to_string())).collect();
        let esquema = json!({"type": "object", "properties": {"name": {"type": "string"}}, "required": ["name"]});
        let v = self.perguntar(&sistema, &ex, &pergunta, esquema, 48)?;
        let n = v["name"].as_str().unwrap_or_default().trim().to_string();
        if n.is_empty() {
            bail!("a IA não deu um nome");
        }
        Ok(n)
    }
}

/// Chave para guardar os termos de uma busca (só as palavras que contam, sem os "-palavra").
pub fn chave_busca(busca: &str) -> Option<String> {
    let ps: Vec<String> = busca.split_whitespace().filter(|t| !t.starts_with('-')).flat_map(palavras).collect();
    (!ps.is_empty()).then(|| format!("busca:{}", ps.join(" ")))
}

/// Chave para guardar o nome dado a um som.
pub fn chave_nome(original: &str, descricao: &str, encurtar: bool) -> String {
    format!("nome:{}:{original}|{descricao}", if encurtar { "curto" } else { "longo" })
}

/// Termos da busca, guardados no índice depois da primeira vez.
pub fn termos_guardados(ind: &crate::indice::Indice, busca: &str) -> Option<Termos> {
    chave_busca(busca).and_then(|k| ind.ia_lembrar(&k)).and_then(|v| serde_json::from_str(&v).ok())
}

/// Pergunta à IA (ou usa o que já está guardado) e guarda a resposta.
pub fn termos_com_memoria(ind: &crate::indice::Indice, ia: &Ia, busca: &str, dic: &Dicionario) -> Result<Termos> {
    if let Some(t) = termos_guardados(ind, busca) {
        return Ok(t);
    }
    let t = ia.termos_busca(busca, dic)?;
    if let Some(k) = chave_busca(busca) {
        ind.ia_guardar(&k, &serde_json::to_string(&t)?)?;
    }
    Ok(t)
}

/// Pistas do dicionário para uma busca em português ("gaveta = drawer").
pub fn pistas_pt(busca: &str, dic: &Dicionario) -> Vec<String> {
    dic.expandir(&palavras(busca))
        .into_iter()
        .filter(|(_, t)| !t.is_empty())
        .map(|(p, t)| format!("{p} = {}", t.iter().take(3).cloned().collect::<Vec<_>>().join(", ")))
        .collect()
}

/// Pistas do dicionário para um nome em inglês ("slam = Batendo"), sem repetir.
pub fn pistas_en(nome: &str, dic: &Dicionario) -> Vec<String> {
    let sep = crate::nomes::palavras_do_nome(nome);
    let ps: Vec<String> = sep.iter().map(|p| p.to_lowercase()).filter(|p| p.chars().count() > 2 && !p.chars().all(|c| c.is_ascii_digit())).collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < ps.len() {
        // expressões de duas palavras primeiro ("fire engine")
        if i + 1 < ps.len() {
            let dupla = format!("{} {}", ps[i], ps[i + 1]);
            if let Some(t) = dic.traduzir_en(&dupla) {
                let l = format!("{dupla} = {t}");
                if !out.contains(&l) {
                    out.push(l);
                }
                i += 2;
                continue;
            }
        }
        if let Some(t) = dic.traduzir_en(&ps[i]) {
            let l = format!("{} = {t}", ps[i]);
            if !out.contains(&l) {
                out.push(l);
            }
        }
        i += 1;
    }
    out
}
