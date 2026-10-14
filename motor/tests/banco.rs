//! Testes do SFX Finder: monta um banco de sons de mentira com o ffmpeg, indexa, busca (em português também),
//! vota, copia e converte para uma pasta temática. Precisam de ffmpeg/ffprobe no PATH.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

use motor::envio::{enviar, Acao, Envio, ItemCesta};
use motor::indice::{Gosto, Lista};
use motor::{Consulta, Dicionario, Ferramentas, Formato, Fps, Indice};

fn ferramentas() -> Ferramentas {
    Ferramentas::localizar(None).expect("os testes precisam de ffmpeg e ffprobe no PATH")
}

fn pasta(nome: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("sfxfinder-banco-{}-{nome}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// Som de teste: `seg` segundos, `canais` canais, no formato da extensão.
fn som(arq: &Path, seg: f64, canais: u32, extra: &[&str]) {
    std::fs::create_dir_all(arq.parent().unwrap()).unwrap();
    let fonte = format!("sine=frequency=440:sample_rate=48000:duration={seg}");
    let mut c = Command::new("ffmpeg");
    c.args(["-hide_banner", "-y", "-v", "error", "-f", "lavfi", "-i", &fonte, "-ac", &canais.to_string()]).args(extra).arg(arq);
    assert!(c.status().unwrap().success(), "ffmpeg falhou: {}", arq.display());
}

/// 01_BIBLIOTECAS/Sony/Home & Office/…, 01_BIBLIOTECAS/BBC/…, 00_INBOX/…
fn banco(raiz: &Path) {
    let b = raiz.join("01_BIBLIOTECAS");
    som(&b.join("Sony/Home & Office/Door Metal Slam 02.wav"), 2.4, 2, &["-c:a", "pcm_s24le"]);
    som(&b.join("Sony/Home & Office/Door Close Slam.wav"), 1.8, 2, &["-c:a", "pcm_s24le"]);
    som(&b.join("Sony/Home & Office/Laundry Dryer Door Slam.wav"), 1.2, 1, &["-c:a", "pcm_s16le"]);
    som(&b.join("BBC/Transport/07045298 Fire engine, door slammed.mp3"), 2.7, 2, &[]);
    som(&b.join("BBC/Nature/Heavy Rain On Roof.flac"), 6.0, 2, &[]);
    som(&b.join("BBC/Nature/DogBarkingDistant.wav"), 3.0, 1, &[]);
    // descrição só nos metadados (título), nome sem sentido
    som(&b.join("BBC/Nature/Track 01.mp3"), 1.0, 2, &["-metadata", "title=Thunder clap close"]);
    som(&raiz.join("00_INBOX/Pissing Urinating Sound Effect.mp3"), 1.5, 2, &[]);
    // ignorados: oculto e não áudio
    som(&raiz.join(".oculta/Door Slam Escondido.wav"), 1.0, 1, &[]);
    std::fs::write(raiz.join("01_BIBLIOTECAS/LEIA.txt"), "door slam").unwrap();
}

fn indexar(ind: &mut Indice, raiz: &Path) -> motor::indice::Resumo {
    ind.atualizar(raiz, &mut |_| {}, &AtomicBool::new(false)).unwrap()
}

fn nomes(r: &motor::indice::Resposta) -> Vec<String> {
    r.sons.iter().map(|s| s.nome.clone()).collect()
}

fn busca(ind: &Indice, texto: &str) -> motor::indice::Resposta {
    ind.buscar(&Consulta { texto: texto.into(), ..Default::default() }, Dicionario::embutido()).unwrap()
}

#[test]
fn indexa_e_busca() {
    let raiz = pasta("indexa");
    banco(&raiz);
    let mut ind = Indice::em_memoria().unwrap();
    let r = indexar(&mut ind, &raiz);
    assert_eq!((r.total, r.novos, r.removidos), (8, 8, 0));
    assert_eq!(ind.total(), 8);

    // inglês, com plural/verbo (slammed) pela raiz da palavra
    let r = busca(&ind, "door slam");
    assert_eq!(r.total, 4, "{:?}", nomes(&r));
    // sem perda antes de MP3
    assert_eq!(r.sons.last().unwrap().ext, "mp3");
    // contagem por biblioteca
    assert!(r.bibliotecas.iter().any(|b| b.biblioteca == "Sony" && b.n == 3));

    // português
    let r = busca(&ind, "porta batendo");
    assert_eq!(r.total, 4, "{:?}", nomes(&r));
    assert!(r.traducoes.iter().any(|t| t.starts_with("door")), "{:?}", r.traducoes);
    assert_eq!(busca(&ind, "chuva").total, 1);
    assert_eq!(busca(&ind, "cachorro latindo").total, 1, "palavras coladas (DogBarking) e verbo");
    assert_eq!(busca(&ind, "trovão").total, 1, "metadados (título)");
    assert_eq!(busca(&ind, "home office").total, 3, "nome da pasta");

    // exclusão e duração
    assert_eq!(busca(&ind, "door slam -metal").total, 3);
    let curta = ind.buscar(&Consulta { texto: "door".into(), dur_max: Some(2.0), ..Default::default() }, Dicionario::embutido()).unwrap();
    assert_eq!(curta.total, 2, "{:?}", nomes(&curta));
    let sem_mp3 = ind.buscar(&Consulta { texto: "door".into(), so_sem_perda: true, ..Default::default() }, Dicionario::embutido()).unwrap();
    assert_eq!(sem_mp3.total, 3);
    let mono = ind.buscar(&Consulta { texto: "door".into(), canais: Some(1), ..Default::default() }, Dicionario::embutido()).unwrap();
    assert_eq!(nomes(&mono), ["Laundry Dryer Door Slam"]);

    // navegar por biblioteca, sem texto
    let sony = ind.buscar(&Consulta { grupo: Some("01_BIBLIOTECAS".into()), biblioteca: Some("Sony".into()), ..Default::default() }, Dicionario::embutido()).unwrap();
    assert_eq!(sony.total, 3);
    assert_eq!(busca(&ind, "").total, 0);

    // ficha do som
    let s = &busca(&ind, "metal").sons[0];
    assert_eq!((s.ext.as_str(), s.sr, s.ch, s.bits), ("wav", Some(48000), Some(2), Some(24)));
    assert!((s.dur.unwrap() - 2.4).abs() < 0.05);
    assert_eq!((s.grupo.as_str(), s.biblioteca.as_str(), s.pasta.as_str()), ("01_BIBLIOTECAS", "Sony", "Home & Office"));
}

#[test]
fn gosto_e_usados() {
    let raiz = pasta("gosto");
    banco(&raiz);
    let mut ind = Indice::em_memoria().unwrap();
    indexar(&mut ind, &raiz);
    let r = busca(&ind, "door slam");
    let mp3 = r.sons.iter().find(|s| s.ext == "mp3").unwrap().caminho.clone();
    let primeiro = r.sons[0].caminho.clone();
    // 👍 sobe para o topo, 👎 desce para o fim
    ind.votar(&mp3, 1).unwrap();
    ind.votar(&primeiro, -1).unwrap();
    let r = busca(&ind, "door slam");
    assert_eq!(r.sons[0].caminho, mp3);
    assert_eq!(r.sons[0].voto, 1);
    assert_eq!(r.sons.last().unwrap().caminho, primeiro);
    let c = |g| Consulta { texto: "door slam".into(), gosto: g, ..Default::default() };
    assert_eq!(ind.buscar(&c(Gosto::SoGostei), Dicionario::embutido()).unwrap().total, 1);
    assert_eq!(ind.buscar(&c(Gosto::SemRuins), Dicionario::embutido()).unwrap().total, 3);
    // usados
    ind.marcar_uso(&mp3, "/x/SFX_EP12/PORTAS").unwrap();
    let r = busca(&ind, "door slam");
    assert_eq!(r.sons[0].usado.as_deref(), Some("/x/SFX_EP12/PORTAS"));
    let rk = ind.ranking();
    assert_eq!((rk.gostei, rk.nao_gostei, rk.usados), (1, 1, 1));
    let usados = ind.buscar(&Consulta { lista: Some(Lista::Usados), ..Default::default() }, Dicionario::embutido()).unwrap();
    assert_eq!(usados.total, 1);
    // tirar o voto
    ind.votar(&mp3, 0).unwrap();
    assert_eq!(ind.ranking().gostei, 0);
}

#[test]
fn atualiza_so_o_que_mudou() {
    let raiz = pasta("atualiza");
    banco(&raiz);
    let mut ind = Indice::em_memoria().unwrap();
    indexar(&mut ind, &raiz);
    let r = indexar(&mut ind, &raiz);
    assert_eq!((r.total, r.novos, r.removidos), (8, 0, 0), "nada mudou: nada é lido de novo");
    std::fs::remove_file(raiz.join("01_BIBLIOTECAS/BBC/Nature/Heavy Rain On Roof.flac")).unwrap();
    som(&raiz.join("00_INBOX/Glass Break.wav"), 1.0, 1, &[]);
    let r = indexar(&mut ind, &raiz);
    assert_eq!((r.total, r.novos, r.removidos), (8, 1, 1));
    assert_eq!(busca(&ind, "chuva").total, 0);
    assert_eq!(busca(&ind, "vidro quebrando").total, 1);
    // outro banco troca o índice
    let outro = pasta("atualiza-outro");
    som(&outro.join("Libs/X/Wind Gust.wav"), 1.0, 1, &[]);
    let r = indexar(&mut ind, &outro);
    assert_eq!((r.total, r.novos), (1, 1));
    assert_eq!(ind.total(), 1);
}

fn envio(pasta: &Path, acao: Acao, formato: Formato) -> Envio {
    Envio { pasta: pasta.to_path_buf(), acao, formato, taxa: Some(48000), bits: 24, kbps: None, fps: Fps::new(25, 1, false), origem: true }
}

#[test]
fn copia_e_converte() {
    let ferr = ferramentas();
    let raiz = pasta("envio");
    banco(&raiz);
    let destino = raiz.join("SFX_EP18/PORTAS");
    let original = raiz.join("01_BIBLIOTECAS/Sony/Home & Office/Door Metal Slam 02.wav");
    let item = ItemCesta { caminho: original.to_string_lossy().into(), nome: "Porta Metal Batendo".into(), trecho: None };
    let nada = AtomicBool::new(false);

    // copiar: o mesmo arquivo, com o nome novo; a segunda vez não sobrescreve
    let r = enviar(&ferr, &item, &envio(&destino, Acao::Copiar, Formato::Wav), &mut |_| {}, &nada).unwrap();
    assert!(r.arquivos[0].ends_with("PORTAS/Porta Metal Batendo.wav"), "{:?}", r.arquivos);
    assert_eq!(std::fs::read(&r.arquivos[0]).unwrap(), std::fs::read(&original).unwrap());
    let r2 = enviar(&ferr, &item, &envio(&destino, Acao::Copiar, Formato::Wav), &mut |_| {}, &nada).unwrap();
    assert!(r2.arquivos[0].ends_with("Porta Metal Batendo_2.wav"));

    // copiar só um trecho
    let t = ItemCesta { trecho: Some([0.5, 1.3]), nome: "Trecho".into(), ..item.clone() };
    let r = enviar(&ferr, &t, &envio(&destino, Acao::Copiar, Formato::Wav), &mut |_| {}, &nada).unwrap();
    let i = motor::inspecionar(&ferr, Path::new(&r.arquivos[0]), Fps::new(25, 1, false)).unwrap();
    assert!((i.duracao_s - 0.8).abs() < 0.02, "duração {}", i.duracao_s);

    // converter MP3 → WAV 48k/24 com o nome novo e um trecho
    let mp3 = raiz.join("01_BIBLIOTECAS/BBC/Transport/07045298 Fire engine, door slammed.mp3");
    let c = ItemCesta { caminho: mp3.to_string_lossy().into(), nome: "Caminhão Bombeiro Porta".into(), trecho: Some([1.0, 2.0]) };
    let r = enviar(&ferr, &c, &envio(&destino, Acao::Converter, Formato::Wav), &mut |_| {}, &nada).unwrap();
    assert!(r.arquivos[0].ends_with("Caminhão Bombeiro Porta.wav"), "{:?}", r.arquivos);
    let i = motor::inspecionar(&ferr, Path::new(&r.arquivos[0]), Fps::new(25, 1, false)).unwrap();
    assert_eq!((i.sr, i.canais), (48000, 2));
    assert!((i.duracao_s - 1.0).abs() < 0.03, "duração {}", i.duracao_s);

    // ORIGEM.txt com uma linha por envio
    let origem = std::fs::read_to_string(destino.join("ORIGEM.txt")).unwrap();
    assert!(origem.contains("Porta Metal Batendo.wav  ←  "), "{origem}");
    assert!(origem.contains("[trecho 1,00 a 2,00 s]"), "{origem}");
    assert_eq!(origem.matches('←').count(), 4);

    assert_eq!(motor::envio::subpastas(&raiz.join("SFX_EP18")), ["PORTAS"]);
}

#[test]
fn converte_mxf() {
    let ferr = ferramentas();
    if ferr.raw2bmx.is_none() {
        eprintln!("sem raw2bmx: teste de MXF pulado");
        return;
    }
    let raiz = pasta("mxf");
    let original = raiz.join("Door Slam.flac");
    som(&original, 1.5, 2, &[]);
    let destino = raiz.join("AVID");
    let item = ItemCesta { caminho: original.to_string_lossy().into(), nome: "Porta Batendo".into(), trecho: None };
    let r = enviar(&ferr, &item, &envio(&destino, Acao::Converter, Formato::Mxf), &mut |_| {}, &AtomicBool::new(false)).unwrap();
    let nomes: Vec<String> = r.arquivos.iter().map(|a| Path::new(a).file_name().unwrap().to_string_lossy().into_owned()).collect();
    assert_eq!(nomes, ["Porta Batendo_L.mxf", "Porta Batendo_R.mxf"]);
}

#[test]
fn forma_de_onda() {
    let ferr = ferramentas();
    let raiz = pasta("onda");
    let arq = raiz.join("bip.wav");
    // meio segundo de som e meio de silêncio
    let s = Command::new("ffmpeg")
        .args(["-hide_banner", "-y", "-v", "error", "-f", "lavfi", "-i", "sine=f=440:d=0.5", "-af", "apad=pad_dur=0.5"])
        .arg(&arq)
        .status()
        .unwrap();
    assert!(s.success());
    let p = motor::envio::picos(&ferr, &arq, 10).unwrap();
    assert_eq!(p.len(), 10);
    assert!(p[..5].iter().all(|&v| v > 200), "{p:?}");
    assert!(p[6..].iter().all(|&v| v < 10), "{p:?}");
    let prev = motor::envio::previa(&ferr, &arq, &raiz.join("cache")).unwrap();
    assert!(prev.is_file());
    assert_eq!(motor::envio::previa(&ferr, &arq, &raiz.join("cache")).unwrap(), prev, "prévia reaproveitada");
}
