//! Linha de comando do motor, para testar sem a interface:
//!   motor-cli indexar PASTA_DO_BANCO                       (lê o banco e guarda o índice do app)
//!   motor-cli buscar "porta batendo" [--max 5] [--sem-perda] [-n 20]
//!   motor-cli nome "Door Metal Slam 02" [-t (traduzir)] [-e (encurtar)]
//!   motor-cli arquivo...                                  (só inspeciona)
//!   motor-cli -f mxf [-r 48000] [-b 24] [-k 320] [-m] [-o pasta] [--nome X] [--trecho 0.5 1.3] arquivo...

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use motor::{Ferramentas, Fps, Opcoes};

const USO: &str = "uso: motor-cli indexar PASTA | buscar TEXTO [--max s] [--sem-perda] [-n N] | nome NOME [-t] [-e]\n     motor-cli [-f formato] [-r taxa] [-b bits] [-k kbps] [-m] [-o pasta] [--nome X] [--trecho ini fim] [--fps 25] arquivo...";

fn indexar(pasta: &str) -> ExitCode {
    let arq = motor::indice::arquivo_padrao();
    let mut ind = match motor::Indice::abrir(&arq) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("ERRO: {e:#}");
            return ExitCode::FAILURE;
        }
    };
    println!("índice: {}", arq.display());
    let r = ind.atualizar(std::path::Path::new(pasta), &mut |p| eprint!("\r{} vistos · {} de {} lidos   ", p.vistos, p.lidos, p.a_ler), &AtomicBool::new(false));
    eprintln!();
    match r {
        Ok(r) => {
            println!("{} sons · {} lidos agora · {} removidos · {:.0} s", r.total, r.novos, r.removidos, r.segundos);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("ERRO: {e:#}");
            ExitCode::FAILURE
        }
    }
}

/// IA para os testes na linha de comando: llama-server das ferramentas e o modelo em app/modelo/qwen.gguf
/// (ou SFXFINDER_MODELO).
fn abrir_ia() -> Option<motor::ia::Ia> {
    let ferr = Ferramentas::localizar(None).ok()?;
    let servidor = ferr.llama?;
    let modelo = std::env::var_os("SFXFINDER_MODELO").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("app/modelo/qwen.gguf"));
    let t0 = std::time::Instant::now();
    match motor::ia::Ia::iniciar(&servidor, &modelo) {
        Ok(ia) => {
            eprintln!("IA aberta em {:.1} s", t0.elapsed().as_secs_f64());
            Some(ia)
        }
        Err(e) => {
            eprintln!("sem IA: {e:#}");
            None
        }
    }
}

fn buscar(args: &[String]) -> ExitCode {
    let mut c = motor::Consulta::default();
    let com_ia = args.iter().any(|a| a == "--ia");
    let mut texto = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--max" => c.dur_max = it.next().and_then(|v| v.parse().ok()),
            "--min" => c.dur_min = it.next().and_then(|v| v.parse().ok()),
            "--sem-perda" => c.so_sem_perda = true,
            "-n" => c.limite = it.next().and_then(|v| v.parse().ok()),
            "--ia" => {}
            _ => texto.push(a.clone()),
        }
    }
    c.texto = texto.join(" ");
    c.limite = c.limite.or(Some(20));
    let ind = match motor::Indice::abrir(&motor::indice::arquivo_padrao()) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("ERRO: {e:#}");
            return ExitCode::FAILURE;
        }
    };
    let ia = if com_ia { abrir_ia() } else { None };
    let dic = motor::Dicionario::embutido();
    let t0 = std::time::Instant::now();
    let termos = ia.as_ref().and_then(|ia| motor::ia::termos_com_memoria(&ind, ia, &c.texto, dic).map_err(|e| eprintln!("IA: {e:#}")).ok());
    let t_ia = t0.elapsed().as_millis();
    match ind.buscar_com_ia(&c, dic, termos.as_ref()) {
        Ok(r) => {
            let ms = t0.elapsed().as_millis();
            if let Some(t) = &termos {
                println!("IA ({t_ia} ms): [{}] {}", t.ingles, t.termos.join(" · "));
            }
            println!("{} encontrados em {ms} ms · também: {}", r.total, r.traducoes.join(" · "));
            for b in r.bibliotecas.iter().take(8) {
                println!("  {:>5}  {} › {}", b.n, b.grupo, b.biblioteca);
            }
            for s in &r.sons {
                println!("{:6.1}s {:>5} {:>6} {}ch {}  [{} › {}]", s.dur.unwrap_or(0.0), s.ext, s.sr.unwrap_or(0), s.ch.unwrap_or(0), s.nome, s.biblioteca, s.pasta);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("ERRO: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    let todos: Vec<String> = std::env::args().skip(1).collect();
    match todos.first().map(String::as_str) {
        Some("indexar") if todos.len() == 2 => return indexar(&todos[1]),
        Some("buscar") => return buscar(&todos[1..]),
        Some("nome") => {
            let t = todos.iter().any(|a| a == "-t");
            let e = todos.iter().any(|a| a == "-e");
            let ia = if todos.iter().any(|a| a == "--ia") { abrir_ia() } else { None };
            // vários nomes separados por " ; "
            let nomes: Vec<&str> = todos[1..].iter().filter(|a| !a.starts_with('-')).map(String::as_str).collect();
            let junto = nomes.join(" ");
            for n in junto.split(" ; ") {
                let d = motor::Dicionario::embutido();
                let via_ia = ia.as_ref().filter(|_| t).and_then(|ia| ia.nome(n, "", e, d).ok());
                println!("{n}  →  {}", via_ia.unwrap_or_else(|| motor::nomes::sugerir(n, t, e, d)));
            }
            return ExitCode::SUCCESS;
        }
        _ => {}
    }
    let mut formato: Option<String> = None;
    let mut op = Opcoes { formato: motor::Formato::Wav, taxa: None, bits: 24, kbps: None, separar: false, destino: None, fps: Fps::new(24000, 1001, false), nome: None, trecho: None };
    let mut arquivos: Vec<PathBuf> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut valor = || args.next().unwrap_or_default();
        match a.as_str() {
            "-f" => formato = Some(valor()),
            "-r" => op.taxa = valor().parse().ok(),
            "-b" => op.bits = valor().parse().unwrap_or(24),
            "-k" => op.kbps = valor().parse().ok(),
            "-m" => op.separar = true,
            "-o" => op.destino = Some(PathBuf::from(valor())),
            "--fps" => op.fps = Fps::ler(&valor(), false).unwrap_or(op.fps),
            "--nome" => op.nome = Some(valor()),
            "--trecho" => {
                let a: f64 = valor().parse().unwrap_or(0.0);
                let b: f64 = valor().parse().unwrap_or(0.0);
                op.trecho = Some([a, b]);
            }
            // usado no teste da interface com dados simulados
            "--json-formatos" => {
                let v: Vec<_> = motor::VELOCIDADES.iter().map(|f| serde_json::json!({ "fps": f, "rotulo": f.rotulo() })).collect();
                println!("{}", serde_json::json!({ "formatos": motor::FORMATOS, "velocidades": v, "versao": env!("CARGO_PKG_VERSION"), "erro": null,
                    "banco": "/Volumes/INSIDE_10TB/03_BIBLIOTECA DE EDIÇÃO/SFX", "bancoExiste": true, "total": 62353, "atualizadoEm": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
                    "indexando": false, "ia": "pronta", "ranking": { "gostei": 38, "naoGostei": 11, "usados": 126 } }));
                return ExitCode::SUCCESS;
            }
            "-h" | "--help" => {
                println!("{USO}");
                return ExitCode::SUCCESS;
            }
            _ => arquivos.push(PathBuf::from(a)),
        }
    }
    if arquivos.is_empty() {
        eprintln!("{USO}");
        return ExitCode::from(2);
    }
    if let Some(f) = &formato {
        match serde_json::from_value(serde_json::Value::String(f.to_lowercase())) {
            Ok(v) => op.formato = v,
            Err(_) => {
                eprintln!("formato desconhecido: {f}");
                return ExitCode::from(2);
            }
        }
    }
    let ferr = match Ferramentas::localizar(None) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("ERRO: {e:#}");
            return ExitCode::FAILURE;
        }
    };
    let cancelar = AtomicBool::new(false);
    let mut ok = true;
    for a in &arquivos {
        let ins = match motor::inspecionar(&ferr, a, op.fps) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("ERRO {}: {e:#}", a.display());
                ok = false;
                continue;
            }
        };
        let tc = ins.inicio.as_ref().map(|i| format!("TC {} @ {}", i.texto, i.fps.rotulo())).unwrap_or_else(|| "sem timecode".into());
        println!("{}: {} · {} canais [{}] · {} Hz · {:.2} s · {tc}", ins.nome, ins.descricao, ins.canais, ins.rotulos.join(" "), ins.sr, ins.duracao_s);
        if formato.is_none() {
            continue;
        }
        match motor::converter(&ferr, &ins, &op, &mut |_| {}, &cancelar) {
            Ok(r) => {
                for f in &r.arquivos {
                    println!("  → {f}");
                }
                for av in &r.avisos {
                    println!("  aviso: {av}");
                }
            }
            Err(e) => {
                eprintln!("ERRO {}: {e:#}", a.display());
                ok = false;
            }
        }
    }
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
