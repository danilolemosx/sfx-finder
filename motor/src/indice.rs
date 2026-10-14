//! Índice do banco de sons (SQLite, guardado na pasta do app). A primeira leitura percorre o banco todo;
//! nas próximas só os arquivos novos ou alterados são lidos de novo (pelo tamanho e pela data).
//! Também guarda o gosto (👍/👎), os sons já usados e as formas de onda.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use lofty::config::ParseOptions;
use lofty::prelude::*;
use lofty::probe::Probe;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::dicionario::{palavras, Dicionario};

/// Extensões de áudio que entram no índice.
pub const EXTENSOES: &[&str] = &["wav", "bwf", "aif", "aiff", "flac", "mp3", "ogg", "m4a", "aac", "wma", "caf", "w64", "opus"];

pub struct Indice {
    con: Connection,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progresso {
    /// Arquivos de áudio encontrados até agora.
    pub vistos: u64,
    /// Total estimado (0 enquanto ainda conta).
    pub total: u64,
    /// Arquivos lidos (novos ou alterados).
    pub lidos: u64,
    /// Quantos precisam ser lidos (0 enquanto ainda conta).
    pub a_ler: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resumo {
    pub total: u64,
    pub novos: u64,
    pub removidos: u64,
    pub segundos: f64,
}

/// O que se sabe de um arquivo de áudio.
#[derive(Debug, Clone, Default)]
struct Ficha {
    dur: Option<f64>,
    sr: Option<u32>,
    ch: Option<u8>,
    bits: Option<u8>,
    kbps: Option<u32>,
    meta: String,
}

fn ler_ficha(caminho: &Path, ext: &str) -> Ficha {
    let mut f = Ficha::default();
    let mut textos: Vec<String> = Vec::new();
    let opcoes = ParseOptions::new().read_cover_art(false);
    if let Ok(arq) = Probe::open(caminho).and_then(|p| p.options(opcoes).guess_file_type().map_err(Into::into)).and_then(|p| p.read()) {
        let p = arq.properties();
        let d = p.duration().as_secs_f64();
        f.dur = (d > 0.0).then_some(d);
        f.sr = p.sample_rate();
        f.ch = p.channels();
        f.bits = p.bit_depth();
        f.kbps = p.audio_bitrate();
        for tag in arq.tags() {
            for chave in [ItemKey::TrackTitle, ItemKey::TrackSubtitle, ItemKey::Comment, ItemKey::Genre, ItemKey::AlbumTitle, ItemKey::Description] {
                for t in tag.get_strings(chave) {
                    let t = t.trim();
                    if !t.is_empty() && t.len() < 500 && !textos.iter().any(|x| x == t) {
                        textos.push(t.to_string());
                    }
                }
            }
        }
    }
    if matches!(ext, "wav" | "bwf") {
        let d = crate::wav::ler_info(caminho).descricao;
        if !d.is_empty() && !textos.contains(&d) {
            textos.push(d);
        }
    }
    let mut meta = textos.join(" | ");
    if meta.len() > 800 {
        let mut corte = 800;
        while !meta.is_char_boundary(corte) {
            corte -= 1;
        }
        meta.truncate(corte);
    }
    f.meta = meta;
    f
}

/// "DoorSlamMetal" → "Door Slam Metal" (para a busca achar palavras coladas).
fn separar_camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    let mut ant: Option<char> = None;
    for c in s.chars() {
        if let Some(a) = ant {
            if (a.is_lowercase() && c.is_uppercase()) || (a.is_alphabetic() && c.is_ascii_digit()) || (a.is_ascii_digit() && c.is_alphabetic()) {
                out.push(' ');
            }
        }
        out.push(c);
        ant = Some(c);
    }
    out
}

fn agora() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Um arquivo encontrado no banco.
struct Achado {
    caminho: PathBuf,
    rel: Vec<String>,
    ext: String,
    tamanho: i64,
    mtime: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Gosto {
    #[default]
    Todos,
    /// Só os 👍.
    SoGostei,
    /// Esconde os 👎.
    SemRuins,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Lista {
    Gostei,
    NaoGostei,
    Usados,
}

/// Uma busca, com os filtros da tela.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Consulta {
    pub texto: String,
    pub dur_min: Option<f64>,
    pub dur_max: Option<f64>,
    /// Esconde MP3 e os outros formatos com perda.
    pub so_sem_perda: bool,
    /// 1 = mono, 2 = estéreo, 3 = mais de 2 canais.
    pub canais: Option<u8>,
    pub gosto: Gosto,
    pub grupo: Option<String>,
    pub biblioteca: Option<String>,
    pub lista: Option<Lista>,
    pub limite: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Som {
    pub caminho: String,
    pub nome: String,
    pub ext: String,
    pub dur: Option<f64>,
    pub sr: Option<u32>,
    pub ch: Option<u8>,
    pub bits: Option<u8>,
    pub kbps: Option<u32>,
    pub grupo: String,
    pub biblioteca: String,
    /// Pasta dentro da biblioteca ("Home & Office").
    pub pasta: String,
    pub meta: String,
    /// 1 gostei, -1 não gostei, 0 nada.
    pub voto: i8,
    /// Última pasta para onde foi copiado/convertido.
    pub usado: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Contagem {
    pub grupo: String,
    pub biblioteca: String,
    pub n: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resposta {
    pub total: u64,
    pub sons: Vec<Som>,
    /// Quantos resultados em cada biblioteca (sem o filtro de biblioteca).
    pub bibliotecas: Vec<Contagem>,
    /// Traduções usadas na busca ("door slam · door bang").
    pub traducoes: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ranking {
    pub gostei: u64,
    pub nao_gostei: u64,
    pub usados: u64,
}

/// Palavras de ligação que não ajudam na busca (fora de expressões como "chave de fenda").
const VAZIAS: &[&str] = &["de", "da", "do", "das", "dos", "a", "o", "as", "os", "e", "em", "no", "na", "nos", "nas", "um", "uma", "com", "para", "por", "the", "of", "and", "an"];

/// Termo da busca entre aspas (para o SQLite não ler como comando).
fn termo(p: &str, prefixo: bool) -> String {
    let limpo = p.replace('"', "");
    if limpo.contains(' ') {
        format!("\"{limpo}\"")
    } else if prefixo && limpo.chars().count() >= 3 {
        format!("\"{limpo}\"*")
    } else {
        format!("\"{limpo}\"")
    }
}

/// Monta a expressão do FTS5: cada grupo vira (palavra* OR tradução OR …), unidos por AND; "-palavra" exclui.
pub fn expressao(texto: &str, dic: &Dicionario) -> (Option<String>, Vec<String>) {
    let mut pos: Vec<String> = Vec::new();
    let mut neg: Vec<String> = Vec::new();
    for t in texto.split_whitespace() {
        match t.strip_prefix('-') {
            Some(n) if !n.is_empty() => neg.extend(palavras(n)),
            _ => pos.extend(palavras(t)),
        }
    }
    let grupos = dic.expandir(&pos);
    let mut partes = Vec::new();
    let mut traducoes: Vec<Vec<String>> = Vec::new();
    for (chave, trad) in &grupos {
        if trad.is_empty() && VAZIAS.contains(&chave.as_str()) {
            continue;
        }
        let mut alts = vec![termo(chave, true)];
        alts.extend(trad.iter().map(|t| termo(t, false)));
        partes.push(format!("({})", alts.join(" OR ")));
        if !trad.is_empty() {
            traducoes.push(trad.iter().take(2).cloned().collect());
        }
    }
    if partes.is_empty() {
        return (None, Vec::new());
    }
    let mut expr = partes.join(" AND ");
    for n in neg {
        expr.push_str(&format!(" NOT {}", termo(&n, true)));
    }
    // "door slam · door knock": a 1ª tradução de cada palavra; depois a 2ª da última palavra que tiver outra
    let mut dicas = Vec::new();
    if !traducoes.is_empty() {
        let base: Vec<&str> = traducoes.iter().map(|t| t[0].as_str()).collect();
        dicas.push(base.join(" "));
        if let Some(k) = traducoes.iter().rposition(|t| t.len() > 1) {
            let mut alt = base.clone();
            alt[k] = traducoes[k][1].as_str();
            dicas.push(alt.join(" "));
        }
    }
    (Some(expr), dicas)
}

/// Onde o app guarda o índice: a pasta de dados do app (a mesma da linha de comando).
pub fn arquivo_padrao() -> PathBuf {
    let id = "com.danilolemos.sfxfinder";
    let base = if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    };
    base.unwrap_or_else(std::env::temp_dir).join(id).join("indice.sqlite")
}

impl Indice {
    pub fn abrir(arquivo: &Path) -> Result<Indice> {
        if let Some(p) = arquivo.parent() {
            std::fs::create_dir_all(p).ok();
        }
        let con = Connection::open(arquivo).with_context(|| format!("não foi possível abrir o índice {}", arquivo.display()))?;
        Self::preparar(con)
    }

    /// Índice temporário (testes).
    pub fn em_memoria() -> Result<Indice> {
        Self::preparar(Connection::open_in_memory()?)
    }

    fn preparar(con: Connection) -> Result<Indice> {
        con.busy_timeout(std::time::Duration::from_secs(10))?;
        con.execute_batch(
            "pragma journal_mode = wal;
             pragma synchronous = normal;
             create table if not exists config (chave text primary key, valor text);
             create table if not exists f (
               id integer primary key, caminho text unique not null, grupo text, biblioteca text, pasta text,
               nome text, ext text, tamanho int, mtime int, dur real, sr int, ch int, bits int, kbps int, meta text,
               picos blob);
             create index if not exists f_bib on f(grupo, biblioteca);
             create virtual table if not exists busca using fts5(nome, pasta, meta, tokenize = 'porter unicode61 remove_diacritics 2');
             create table if not exists votos (caminho text primary key, voto int not null);
             create table if not exists usos (caminho text not null, destino text not null, quando int not null);
             create index if not exists usos_c on usos(caminho);",
        )?;
        Ok(Indice { con })
    }

    fn config(&self, chave: &str) -> Option<String> {
        self.con.query_row("select valor from config where chave = ?", [chave], |r| r.get(0)).optional().ok().flatten()
    }

    fn set_config(&self, chave: &str, valor: &str) -> Result<()> {
        self.con.execute("insert or replace into config values (?, ?)", [chave, valor])?;
        Ok(())
    }

    /// Pasta do banco indexado por último.
    pub fn raiz(&self) -> Option<String> {
        self.config("raiz")
    }

    /// Data (segundos desde 1970) da última atualização completa.
    pub fn atualizado_em(&self) -> Option<i64> {
        self.config("atualizado").and_then(|v| v.parse().ok())
    }

    pub fn total(&self) -> u64 {
        self.con.query_row("select count(*) from f", [], |r| r.get::<_, i64>(0)).unwrap_or(0) as u64
    }

    /// Lê o banco em `raiz`: inclui os arquivos novos, relê os alterados e tira os que sumiram.
    /// Se `raiz` for outra pasta, o índice anterior é trocado (o gosto e os usos ficam).
    pub fn atualizar(&mut self, raiz: &Path, progresso: &mut dyn FnMut(Progresso), cancelar: &AtomicBool) -> Result<Resumo> {
        let t0 = Instant::now();
        if !raiz.is_dir() {
            bail!("a pasta do banco não existe: {}", raiz.display());
        }
        let raiz_txt = raiz.to_string_lossy().into_owned();
        if self.raiz().as_deref() != Some(raiz_txt.as_str()) {
            self.con.execute_batch("delete from f; delete from busca;")?;
            self.set_config("raiz", &raiz_txt)?;
            self.con.execute("delete from config where chave = 'atualizado'", [])?;
        }

        // o que já está no índice
        let mut conhecidos: HashMap<String, (i64, i64, i64)> = HashMap::new();
        {
            let mut st = self.con.prepare("select caminho, tamanho, mtime, id from f")?;
            let linhas = st.query_map([], |r| Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?, r.get(3)?))))?;
            for l in linhas {
                let (c, v) = l?;
                conhecidos.insert(c, v);
            }
        }
        let estimado = conhecidos.len() as u64;

        // 1. percorre as pastas
        let mut p = Progresso { total: estimado, ..Default::default() };
        let mut achados: Vec<Achado> = Vec::new();
        let mut vistos: std::collections::HashSet<String> = std::collections::HashSet::new();
        let andar = walkdir::WalkDir::new(raiz).follow_links(false).into_iter().filter_entry(|e| {
            let n = e.file_name().to_string_lossy();
            e.depth() == 0 || !(n.starts_with('.') || (e.file_type().is_dir() && n.starts_with('$')))
        });
        for e in andar {
            if cancelar.load(Ordering::Relaxed) {
                bail!("cancelado");
            }
            let Ok(e) = e else { continue };
            if !e.file_type().is_file() {
                continue;
            }
            let caminho = e.path();
            let ext = caminho.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
            if !EXTENSOES.contains(&ext.as_str()) {
                continue;
            }
            let Ok(md) = e.metadata() else { continue };
            let mtime = md.modified().ok().and_then(|m| m.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
            let tamanho = md.len() as i64;
            let txt = caminho.to_string_lossy().into_owned();
            p.vistos += 1;
            if p.vistos % 500 == 0 {
                progresso(p);
            }
            vistos.insert(txt.clone());
            if conhecidos.get(&txt).is_some_and(|k| k.0 == tamanho && k.1 == mtime) {
                continue;
            }
            let rel: Vec<String> = caminho
                .strip_prefix(raiz)
                .map(|r| r.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect())
                .unwrap_or_default();
            achados.push(Achado { caminho: caminho.to_path_buf(), rel, ext, tamanho, mtime });
        }

        // 2. tira os que sumiram
        let sumiram: Vec<i64> = conhecidos.iter().filter(|(c, _)| !vistos.contains(*c)).map(|(_, v)| v.2).collect();
        {
            let tx = self.con.transaction()?;
            for id in &sumiram {
                tx.execute("delete from f where id = ?", [id])?;
                tx.execute("delete from busca where rowid = ?", [id])?;
            }
            tx.commit()?;
        }

        // 3. lê os novos e alterados (4 leitores em paralelo; o disco é o limite)
        p.total = p.vistos;
        p.a_ler = achados.len() as u64;
        progresso(p);
        let (tx_fichas, rx_fichas) = mpsc::sync_channel::<(usize, Ficha)>(256);
        let n_leitores = 4;
        let achados_ref = &achados;
        let resultado: Result<()> = std::thread::scope(|s| {
            for k in 0..n_leitores {
                let tx = tx_fichas.clone();
                s.spawn(move || {
                    for (i, a) in achados_ref.iter().enumerate().skip(k).step_by(n_leitores) {
                        if cancelar.load(Ordering::Relaxed) {
                            break;
                        }
                        if tx.send((i, ler_ficha(&a.caminho, &a.ext))).is_err() {
                            break;
                        }
                    }
                });
            }
            drop(tx_fichas);
            let mut lote = 0;
            let mut transacao = self.con.unchecked_transaction()?;
            for (i, ficha) in rx_fichas {
                let a = &achados[i];
                gravar(&transacao, a, &ficha, conhecidos.get(&a.caminho.to_string_lossy().into_owned()).map(|k| k.2))?;
                p.lidos += 1;
                lote += 1;
                if lote >= 300 {
                    transacao.commit()?;
                    transacao = self.con.unchecked_transaction()?;
                    lote = 0;
                    progresso(p);
                }
            }
            transacao.commit()?;
            Ok(())
        });
        resultado?;
        if cancelar.load(Ordering::Relaxed) {
            bail!("cancelado");
        }
        self.set_config("atualizado", &agora().to_string())?;
        progresso(p);
        Ok(Resumo { total: p.vistos, novos: achados.len() as u64, removidos: sumiram.len() as u64, segundos: t0.elapsed().as_secs_f64() })
    }

    /// Todas as bibliotecas do banco, com o número de sons (para navegar sem buscar).
    pub fn bibliotecas(&self) -> Result<Vec<Contagem>> {
        let mut st = self.con.prepare("select grupo, biblioteca, count(*) from f group by grupo, biblioteca order by grupo, biblioteca")?;
        let linhas = st.query_map([], |r| {
            Ok(Contagem { grupo: r.get::<_, Option<String>>(0)?.unwrap_or_default(), biblioteca: r.get::<_, Option<String>>(1)?.unwrap_or_default(), n: r.get::<_, i64>(2)? as u64 })
        })?;
        Ok(linhas.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn votar(&self, caminho: &str, voto: i8) -> Result<()> {
        if voto == 0 {
            self.con.execute("delete from votos where caminho = ?", [caminho])?;
        } else {
            self.con.execute("insert or replace into votos values (?, ?)", params![caminho, voto.signum()])?;
        }
        Ok(())
    }

    pub fn marcar_uso(&self, caminho: &str, destino: &str) -> Result<()> {
        self.con.execute("insert into usos values (?, ?, ?)", params![caminho, destino, agora()])?;
        Ok(())
    }

    pub fn ranking(&self) -> Ranking {
        let n = |sql: &str| self.con.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap_or(0) as u64;
        Ranking {
            gostei: n("select count(*) from votos v join f on f.caminho = v.caminho where v.voto > 0"),
            nao_gostei: n("select count(*) from votos v join f on f.caminho = v.caminho where v.voto < 0"),
            usados: n("select count(distinct u.caminho) from usos u join f on f.caminho = u.caminho"),
        }
    }

    pub fn picos(&self, caminho: &str) -> Option<Vec<u8>> {
        self.con.query_row("select picos from f where caminho = ?", [caminho], |r| r.get::<_, Option<Vec<u8>>>(0)).optional().ok().flatten().flatten()
    }

    pub fn guardar_picos(&self, caminho: &str, picos: &[u8]) -> Result<()> {
        self.con.execute("update f set picos = ? where caminho = ?", params![picos, caminho])?;
        Ok(())
    }

    /// Busca com os filtros. Ordem: 👍 primeiro, 👎 por último; depois sem perda antes de MP3; depois relevância.
    pub fn buscar(&self, c: &Consulta, dic: &Dicionario) -> Result<Resposta> {
        let (expr, traducoes) = expressao(&c.texto, dic);
        if expr.is_none() && c.lista.is_none() && c.biblioteca.is_none() && c.grupo.is_none() {
            return Ok(Resposta::default());
        }
        let mut onde: Vec<String> = Vec::new();
        let mut args: Vec<rusqlite::types::Value> = Vec::new();
        let de = if let Some(e) = &expr {
            onde.push("busca match ?".into());
            args.push(e.clone().into());
            "busca join f on f.id = busca.rowid"
        } else {
            "f"
        };
        if let Some(v) = c.dur_min {
            onde.push("f.dur >= ?".into());
            args.push(v.into());
        }
        if let Some(v) = c.dur_max {
            onde.push("f.dur <= ?".into());
            args.push(v.into());
        }
        if c.so_sem_perda {
            onde.push("f.ext in ('wav','bwf','aif','aiff','caf','w64','flac')".into());
        }
        match c.canais {
            Some(1) => onde.push("f.ch = 1".into()),
            Some(2) => onde.push("f.ch = 2".into()),
            Some(3) => onde.push("f.ch > 2".into()),
            _ => {}
        }
        let voto = "coalesce((select voto from votos where caminho = f.caminho), 0)";
        match c.gosto {
            Gosto::SoGostei => onde.push(format!("{voto} > 0")),
            Gosto::SemRuins => onde.push(format!("{voto} >= 0")),
            Gosto::Todos => {}
        }
        match c.lista {
            Some(Lista::Gostei) => onde.push(format!("{voto} > 0")),
            Some(Lista::NaoGostei) => onde.push(format!("{voto} < 0")),
            Some(Lista::Usados) => onde.push("exists (select 1 from usos where caminho = f.caminho)".into()),
            None => {}
        }
        // contagem por biblioteca (antes do filtro de biblioteca)
        let where_sql = |o: &[String]| if o.is_empty() { String::new() } else { format!("where {}", o.join(" and ")) };
        let mut bibliotecas = Vec::new();
        {
            let sql = format!("select f.grupo, f.biblioteca, count(*) from {de} {} group by f.grupo, f.biblioteca order by count(*) desc", where_sql(&onde));
            let mut st = self.con.prepare(&sql)?;
            let linhas = st.query_map(rusqlite::params_from_iter(args.iter()), |r| {
                Ok(Contagem { grupo: r.get::<_, Option<String>>(0)?.unwrap_or_default(), biblioteca: r.get::<_, Option<String>>(1)?.unwrap_or_default(), n: r.get::<_, i64>(2)? as u64 })
            })?;
            for l in linhas {
                bibliotecas.push(l?);
            }
        }
        if let Some(g) = &c.grupo {
            onde.push("f.grupo = ?".into());
            args.push(g.clone().into());
        }
        if let Some(b) = &c.biblioteca {
            onde.push("f.biblioteca = ?".into());
            args.push(b.clone().into());
        }
        let total = match (&c.grupo, &c.biblioteca) {
            (None, None) => bibliotecas.iter().map(|b| b.n).sum(),
            _ => bibliotecas
                .iter()
                .filter(|b| c.grupo.as_ref().is_none_or(|g| *g == b.grupo) && c.biblioteca.as_ref().is_none_or(|x| *x == b.biblioteca))
                .map(|b| b.n)
                .sum(),
        };
        let relevancia = if expr.is_some() { "bm25(busca, 10.0, 2.0, 1.0)" } else { "f.nome" };
        let qual = "case when f.ext in ('wav','bwf','aif','aiff','caf','w64') then 0 when f.ext = 'flac' then 1 else 2 end";
        let sql = format!(
            "select f.caminho, f.nome, f.ext, f.dur, f.sr, f.ch, f.bits, f.kbps, f.grupo, f.biblioteca, f.pasta, f.meta, {voto},
                    (select destino from usos where caminho = f.caminho order by quando desc limit 1)
             from {de} {} order by {voto} desc, {qual}, {relevancia} limit ?",
            where_sql(&onde)
        );
        args.push((c.limite.unwrap_or(500) as i64).into());
        let mut st = self.con.prepare(&sql)?;
        let linhas = st.query_map(rusqlite::params_from_iter(args.iter()), |r| {
            Ok(Som {
                caminho: r.get(0)?,
                nome: r.get(1)?,
                ext: r.get(2)?,
                dur: r.get(3)?,
                sr: r.get(4)?,
                ch: r.get(5)?,
                bits: r.get(6)?,
                kbps: r.get(7)?,
                grupo: r.get::<_, Option<String>>(8)?.unwrap_or_default(),
                biblioteca: r.get::<_, Option<String>>(9)?.unwrap_or_default(),
                pasta: r.get::<_, Option<String>>(10)?.unwrap_or_default(),
                meta: r.get::<_, Option<String>>(11)?.unwrap_or_default(),
                voto: r.get::<_, i64>(12)? as i8,
                usado: r.get(13)?,
            })
        })?;
        let mut sons = Vec::new();
        for l in linhas {
            sons.push(l?);
        }
        Ok(Resposta { total, sons, bibliotecas, traducoes })
    }
}

/// Grava (ou regrava) um arquivo no índice.
fn gravar(con: &Connection, a: &Achado, f: &Ficha, id_antigo: Option<i64>) -> Result<()> {
    if let Some(id) = id_antigo {
        con.execute("delete from f where id = ?", [id])?;
        con.execute("delete from busca where rowid = ?", [id])?;
    }
    // grupo = 1ª pasta, biblioteca = 2ª pasta, pasta = o resto (sem o nome do arquivo)
    let pastas = &a.rel[..a.rel.len().saturating_sub(1)];
    let grupo = pastas.first().cloned().unwrap_or_default();
    let biblioteca = pastas.get(1).cloned().unwrap_or_default();
    let pasta = pastas.iter().skip(2).cloned().collect::<Vec<_>>().join(" › ");
    let nome = a.caminho.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let caminho = a.caminho.to_string_lossy();
    con.execute(
        "insert into f (caminho, grupo, biblioteca, pasta, nome, ext, tamanho, mtime, dur, sr, ch, bits, kbps, meta)
         values (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![caminho, grupo, biblioteca, pasta, nome, a.ext, a.tamanho, a.mtime, f.dur, f.sr, f.ch, f.bits, f.kbps, f.meta],
    )?;
    let id = con.last_insert_rowid();
    let texto_nome = format!("{nome} {}", separar_camel(&nome));
    let texto_pasta = pastas.join(" ");
    con.execute("insert into busca (rowid, nome, pasta, meta) values (?, ?, ?, ?)", params![id, texto_nome, texto_pasta, f.meta])?;
    Ok(())
}
