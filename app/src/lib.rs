//! Backend Tauri: comandos chamados pela interface (ui/app.js). A lógica fica no crate `motor`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::Manager;

use motor::envio::{Enviado, Envio, ItemCesta};
use motor::indice::{Progresso, Ranking, Resposta, Resumo};
use motor::{Consulta, Dicionario, Ferramentas, Fps, Indice};

/// Ferramentas embutidas (ffmpeg, ffprobe, raw2bmx), procuradas ao lado do executável.
static FERRAMENTAS: OnceLock<Result<Ferramentas, String>> = OnceLock::new();
/// Índice usado nas buscas (a leitura do banco abre outra conexão, para não travar a busca).
static INDICE: Mutex<Option<Indice>> = Mutex::new(None);
static INDEXANDO: AtomicBool = AtomicBool::new(false);
static CANCELAR_INDICE: AtomicBool = AtomicBool::new(false);
static CANCELAR_ENVIO: AtomicBool = AtomicBool::new(false);
/// Cada nova busca muda a geração; os pedidos de forma de onda antigos param.
static GERACAO_ONDAS: AtomicU64 = AtomicU64::new(0);

fn ferramentas() -> Result<&'static Ferramentas, String> {
    FERRAMENTAS
        .get_or_init(|| {
            let pasta = std::env::current_exe().ok().and_then(|e| e.parent().map(PathBuf::from));
            Ferramentas::localizar(pasta.as_deref()).map_err(|e| format!("{e:#}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn erro(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

/// Roda `f` com o índice aberto.
fn com_indice<T>(f: impl FnOnce(&Indice) -> Result<T, String>) -> Result<T, String> {
    let mut g = INDICE.lock().map_err(erro)?;
    if g.is_none() {
        *g = Some(Indice::abrir(&motor::indice::arquivo_padrao()).map_err(erro)?);
    }
    f(g.as_ref().unwrap())
}

#[derive(Serialize)]
struct Velocidade {
    fps: Fps,
    rotulo: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Estado {
    versao: &'static str,
    /// Mensagem quando falta algum programa embutido.
    erro: Option<String>,
    formatos: &'static [motor::InfoFormato],
    velocidades: Vec<Velocidade>,
    /// Pasta do banco de sons (None na primeira vez).
    banco: Option<String>,
    banco_existe: bool,
    total: u64,
    /// Data da última leitura completa (segundos desde 1970).
    atualizado_em: Option<i64>,
    indexando: bool,
    ranking: Ranking,
}

#[tauri::command]
fn estado() -> Result<Estado, String> {
    let erro = match ferramentas() {
        Err(e) => Some(e),
        Ok(f) if f.raw2bmx.is_none() => Some("raw2bmx não encontrado (sem ele não dá para gravar MXF)".into()),
        Ok(_) => None,
    };
    com_indice(|ind| {
        let banco = ind.raiz();
        Ok(Estado {
            versao: env!("CARGO_PKG_VERSION"),
            erro,
            formatos: motor::FORMATOS,
            velocidades: motor::VELOCIDADES.iter().map(|f| Velocidade { fps: *f, rotulo: f.rotulo() }).collect(),
            banco_existe: banco.as_ref().is_some_and(|b| Path::new(b).is_dir()),
            banco,
            total: ind.total(),
            atualizado_em: ind.atualizado_em(),
            indexando: INDEXANDO.load(Ordering::Relaxed),
            ranking: ind.ranking(),
        })
    })
}

/// Lê (ou relê) o banco de sons, mandando o progresso pelo canal.
#[tauri::command]
async fn indexar(pasta: String, progresso: Channel<Progresso>) -> Result<Resumo, String> {
    if INDEXANDO.swap(true, Ordering::SeqCst) {
        return Err("o banco já está sendo lido".into());
    }
    CANCELAR_INDICE.store(false, Ordering::Relaxed);
    let r = tauri::async_runtime::spawn_blocking(move || {
        let mut ind = Indice::abrir(&motor::indice::arquivo_padrao()).map_err(erro)?;
        let mut ultimo = std::time::Instant::now();
        let mut enviar = |p: Progresso| {
            if ultimo.elapsed().as_millis() >= 150 || (p.a_ler > 0 && p.lidos == p.a_ler) {
                ultimo = std::time::Instant::now();
                let _ = progresso.send(p);
            }
        };
        ind.atualizar(Path::new(&pasta), &mut enviar, &CANCELAR_INDICE).map_err(erro)
    })
    .await
    .map_err(erro);
    INDEXANDO.store(false, Ordering::SeqCst);
    r?
}

#[tauri::command]
fn cancelar_indice() {
    CANCELAR_INDICE.store(true, Ordering::Relaxed);
}

#[tauri::command]
async fn buscar(consulta: Consulta) -> Result<Resposta, String> {
    GERACAO_ONDAS.fetch_add(1, Ordering::SeqCst);
    tauri::async_runtime::spawn_blocking(move || com_indice(|ind| ind.buscar(&consulta, Dicionario::embutido()).map_err(erro)))
        .await
        .map_err(erro)?
}

#[tauri::command]
fn bibliotecas() -> Result<Vec<motor::indice::Contagem>, String> {
    com_indice(|ind| ind.bibliotecas().map_err(erro))
}

#[tauri::command]
fn votar(caminho: String, voto: i8) -> Result<Ranking, String> {
    com_indice(|ind| {
        ind.votar(&caminho, voto).map_err(erro)?;
        Ok(ind.ranking())
    })
}

#[derive(Serialize, Clone)]
struct Onda {
    caminho: String,
    picos: Vec<u8>,
}

/// Formas de onda (200 picos cada), mandadas uma a uma pelo canal assim que ficam prontas.
#[tauri::command]
async fn ondas(caminhos: Vec<String>, canal: Channel<Onda>) -> Result<(), String> {
    let geracao = GERACAO_ONDAS.load(Ordering::SeqCst);
    tauri::async_runtime::spawn_blocking(move || {
        let ferr = ferramentas()?;
        for c in caminhos {
            if GERACAO_ONDAS.load(Ordering::SeqCst) != geracao {
                break;
            }
            let guardado = com_indice(|ind| Ok(ind.picos(&c))).ok().flatten();
            let picos = match guardado {
                Some(p) => p,
                None => match motor::envio::picos(ferr, Path::new(&c), 200) {
                    Ok(p) => {
                        let _ = com_indice(|ind| ind.guardar_picos(&c, &p).map_err(erro));
                        p
                    }
                    Err(_) => continue,
                },
            };
            let _ = canal.send(Onda { caminho: c, picos });
        }
        Ok(())
    })
    .await
    .map_err(erro)?
}

/// WAV para ouvir quando o player da janela não toca o formato original.
#[tauri::command]
async fn previa(app: tauri::AppHandle, caminho: String) -> Result<String, String> {
    let cache = app.path().app_cache_dir().map_err(erro)?.join("previas");
    tauri::async_runtime::spawn_blocking(move || {
        motor::envio::previa(ferramentas()?, Path::new(&caminho), &cache).map(|p| p.to_string_lossy().into_owned()).map_err(erro)
    })
    .await
    .map_err(erro)?
}

#[tauri::command]
fn pasta_existe(pasta: String) -> bool {
    Path::new(&pasta).is_dir()
}

#[tauri::command]
fn subpastas(pasta: String) -> Vec<String> {
    motor::envio::subpastas(Path::new(&pasta))
}

#[tauri::command]
fn criar_subpasta(pasta: String, nome: String) -> Result<String, String> {
    let nome = motor::nomes::seguro(&nome);
    std::fs::create_dir_all(Path::new(&pasta).join(&nome)).map_err(erro)?;
    Ok(nome)
}

#[tauri::command]
fn sugerir_nomes(nomes: Vec<String>, traduzir: bool, encurtar: bool) -> Vec<String> {
    let d = Dicionario::embutido();
    nomes.iter().map(|n| motor::nomes::sugerir(n, traduzir, encurtar, d)).collect()
}

#[derive(Serialize, Clone)]
struct ProgressoEnvio {
    item: usize,
    p: f64,
}

/// Copia ou converte os sons da cesta, um por vez; anota cada um como usado.
#[tauri::command]
async fn enviar(itens: Vec<ItemCesta>, envio: Envio, progresso: Channel<ProgressoEnvio>) -> Result<Vec<Enviado>, String> {
    CANCELAR_ENVIO.store(false, Ordering::Relaxed);
    tauri::async_runtime::spawn_blocking(move || {
        let ferr = ferramentas()?;
        let destino = envio.pasta.to_string_lossy().into_owned();
        let mut res = Vec::new();
        for (i, item) in itens.iter().enumerate() {
            if CANCELAR_ENVIO.load(Ordering::Relaxed) {
                break;
            }
            let _ = progresso.send(ProgressoEnvio { item: i, p: 0.0 });
            let mut ultimo = -1.0;
            let mut enviar_p = |p: f64| {
                if p - ultimo >= 0.02 || p >= 1.0 {
                    ultimo = p;
                    let _ = progresso.send(ProgressoEnvio { item: i, p });
                }
            };
            match motor::envio::enviar(ferr, item, &envio, &mut enviar_p, &CANCELAR_ENVIO) {
                Ok(r) => {
                    let _ = com_indice(|ind| ind.marcar_uso(&item.caminho, &destino).map_err(erro));
                    res.push(r);
                }
                Err(e) => res.push(Enviado { caminho: item.caminho.clone(), erro: Some(erro(e)), ..Default::default() }),
            }
        }
        Ok(res)
    })
    .await
    .map_err(erro)?
}

#[tauri::command]
fn cancelar_envio() {
    CANCELAR_ENVIO.store(true, Ordering::Relaxed);
}

/// Ícone mostrado ao arrastar um som para fora da janela (PNG em base64).
#[tauri::command]
fn icone_arraste() -> String {
    const PNG: &[u8] = include_bytes!("../icons/64x64.png");
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::from("data:image/png;base64,");
    for c in PNG.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            s.push(if k <= c.len() { B64[(n >> (18 - 6 * k) & 63) as usize] as char } else { '=' });
        }
    }
    s
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_drag::init())
        .invoke_handler(tauri::generate_handler![
            estado,
            indexar,
            cancelar_indice,
            buscar,
            bibliotecas,
            votar,
            ondas,
            previa,
            pasta_existe,
            subpastas,
            criar_subpasta,
            sugerir_nomes,
            enviar,
            cancelar_envio,
            icone_arraste
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o aplicativo");
}
