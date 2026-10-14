//! Testes de ponta a ponta: geram arquivos com o ffmpeg, convertem com o motor e conferem o resultado.
//! Precisam de ffmpeg/ffprobe no PATH (e de raw2bmx para os testes de MXF).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

use motor::{Ferramentas, Formato, Fps, Opcoes, FORMATOS};

const F23: Fps = Fps::new(24000, 1001, false);

fn ferramentas() -> Ferramentas {
    Ferramentas::localizar(None).expect("os testes precisam de ffmpeg e ffprobe no PATH")
}

fn pasta(nome: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("sfxfinder-teste-{}-{nome}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn ffmpeg(args: &[&str]) {
    let s = Command::new("ffmpeg").args(["-hide_banner", "-y", "-v", "error"]).args(args).status().unwrap();
    assert!(s.success(), "ffmpeg falhou: {args:?}");
}

/// MOV com vídeo, áudio 5.1 (amplitude 0,1 no L até 0,6 no Rs) e timecode 01:02:14:08 a 23,976.
fn mov_51(p: &Path) -> PathBuf {
    let arq = p.join("EP18_CENA01.mov");
    let canais: Vec<String> = (1..=6).map(|k| format!("0.{k}*sin(2*PI*440*t)")).collect();
    let fonte = format!("aevalsrc={}:s=48000:c=5.1:d=2", canais.join("|"));
    let s = Command::new("ffmpeg")
        .args(["-hide_banner", "-y", "-v", "error", "-f", "lavfi", "-i", "color=c=black:s=64x36:r=24000/1001:d=2", "-f", "lavfi", "-i", &fonte])
        .args(["-c:v", "mpeg4", "-c:a", "pcm_s24le", "-timecode", "01:02:14:08"])
        .arg(&arq)
        .status()
        .unwrap();
    assert!(s.success());
    arq
}

fn op(formato: Formato) -> Opcoes {
    Opcoes { formato, taxa: None, bits: 24, kbps: None, separar: false, destino: None, fps: F23, nome: None, trecho: None }
}

fn converter(ferr: &Ferramentas, arq: &Path, o: &Opcoes) -> motor::Resultado {
    let ins = motor::inspecionar(ferr, arq, o.fps).unwrap();
    motor::converter(ferr, &ins, o, &mut |_| {}, &AtomicBool::new(false)).unwrap()
}

fn pico(arq: &str) -> f32 {
    let mut r = hound::WavReader::open(arq).unwrap();
    let escala = (1i64 << (r.spec().bits_per_sample - 1)) as f32;
    r.samples::<i32>().map(|s| (s.unwrap() as f32 / escala).abs()).fold(0.0, f32::max)
}

#[test]
fn inspeciona_mov_51_com_timecode() {
    let ferr = ferramentas();
    let p = pasta("insp51");
    let arq = mov_51(&p);
    let i = motor::inspecionar(&ferr, &arq, Fps::new(25, 1, false)).unwrap();
    assert!(i.video);
    assert_eq!(i.canais, 6);
    assert_eq!(i.descricao, "5.1");
    assert_eq!(i.rotulos, ["L", "R", "C", "LFE", "Ls", "Rs"]);
    assert_eq!(i.sr, 48_000);
    let tc = i.inicio.unwrap();
    assert_eq!(tc.texto, "01:02:14:08");
    assert_eq!(tc.fps, F23);
    assert!(tc.fps_do_arquivo);
}

#[test]
fn separa_51_em_seis_monos_wav_com_timecode() {
    let ferr = ferramentas();
    let p = pasta("monos51");
    let arq = mov_51(&p);
    let r = converter(&ferr, &arq, &Opcoes { separar: true, ..op(Formato::Wav) });
    let nomes: Vec<String> = r.arquivos.iter().map(|a| Path::new(a).file_name().unwrap().to_string_lossy().into_owned()).collect();
    assert_eq!(nomes, ["EP18_CENA01_L.wav", "EP18_CENA01_R.wav", "EP18_CENA01_C.wav", "EP18_CENA01_LFE.wav", "EP18_CENA01_Ls.wav", "EP18_CENA01_Rs.wav"]);
    let quadro = F23.tc_para_quadros("01:02:14:08").unwrap();
    for (k, a) in r.arquivos.iter().enumerate() {
        let spec = hound::WavReader::open(a).unwrap().spec();
        assert_eq!((spec.channels, spec.sample_rate, spec.bits_per_sample), (1, 48_000, 24));
        // cada mono tem o canal certo (amplitudes 0,1…0,6)
        let esperado = (k + 1) as f32 / 10.0;
        assert!((pico(a) - esperado).abs() < 0.01, "{a}: pico {} (esperado {esperado})", pico(a));
        // timecode exato: 2002 amostras por quadro a 23,976
        assert_eq!(motor::wav::ler_info(Path::new(a)).time_reference, Some(quadro * 2002));
    }
}

#[test]
fn mp3_de_51_vira_estereo_com_aviso() {
    let ferr = ferramentas();
    let p = pasta("mp3");
    let arq = mov_51(&p);
    let r = converter(&ferr, &arq, &Opcoes { taxa: Some(96_000), ..op(Formato::Mp3) });
    assert_eq!(r.arquivos.len(), 1);
    assert!(r.arquivos[0].ends_with("EP18_CENA01.mp3"));
    let i = motor::inspecionar(&ferr, Path::new(&r.arquivos[0]), F23).unwrap();
    assert_eq!((i.canais, i.sr), (2, 48_000));
    assert_eq!(r.avisos.len(), 2, "{:?}", r.avisos);
}

#[test]
fn nao_sobrescreve() {
    let ferr = ferramentas();
    let p = pasta("sobrescrever");
    let arq = p.join("a.wav");
    ffmpeg(&["-f", "lavfi", "-i", "sine=f=1000:d=1:r=44100", "-c:a", "pcm_s16le", arq.to_str().unwrap()]);
    let r = converter(&ferr, &arq, &op(Formato::Wav));
    assert!(r.arquivos[0].ends_with("a_2.wav"));
    assert!(arq.exists());
}

/// Acrescenta um bloco ao fim do WAV e corrige o tamanho do RIFF.
fn anexar_bloco(arq: &Path, id: &[u8; 4], dados: &[u8]) {
    let mut b = std::fs::read(arq).unwrap();
    b.extend_from_slice(id);
    b.extend_from_slice(&(dados.len() as u32).to_le_bytes());
    b.extend_from_slice(dados);
    if dados.len() % 2 == 1 {
        b.push(0);
    }
    let riff = (b.len() - 8) as u32;
    b[4..8].copy_from_slice(&riff.to_le_bytes());
    std::fs::write(arq, b).unwrap();
}

#[test]
fn wav_de_gravador_com_bext_e_ixml() {
    let ferr = ferramentas();
    let p = pasta("gravador");
    let arq = p.join("T04.wav");
    // polifônico de 3 canais a 96 kHz, TC 14:22:05:11 a 25 qps
    let fps25 = Fps::new(25, 1, false);
    let tr = fps25.tc_para_quadros("14:22:05:11").unwrap() * 96_000 / 25;
    ffmpeg(&[
        "-f", "lavfi", "-i", "aevalsrc=0.1*sin(2*PI*300*t)|0.2*sin(2*PI*300*t)|0.3*sin(2*PI*300*t):s=96000:d=1",
        "-c:a", "pcm_s24le", "-write_bext", "1", "-metadata", &format!("time_reference={tr}"), arq.to_str().unwrap(),
    ]);
    anexar_bloco(&arq, b"iXML", motor::wav::ixml_exemplo("25", false, &["BOOM", "LAV 1", "LAV 2"]).as_bytes());

    let i = motor::inspecionar(&ferr, &arq, F23).unwrap();
    assert_eq!(i.descricao, "Poli · 3");
    assert_eq!(i.rotulos, ["BOOM", "LAV_1", "LAV_2"]);
    let tc = i.inicio.as_ref().unwrap();
    assert_eq!((tc.texto.as_str(), tc.fps_do_arquivo), ("14:22:05:11", true));

    // para 48 kHz em monos: o timecode acompanha a nova taxa
    let r = converter(&ferr, &arq, &Opcoes { separar: true, taxa: Some(48_000), bits: 16, ..op(Formato::Wav) });
    assert_eq!(r.arquivos.len(), 3);
    assert!(r.arquivos[0].ends_with("T04_BOOM.wav"));
    for a in &r.arquivos {
        assert_eq!(motor::wav::ler_info(Path::new(a)).time_reference, Some(tr / 2));
        let spec = hound::WavReader::open(a).unwrap().spec();
        assert_eq!((spec.sample_rate, spec.bits_per_sample), (48_000, 16));
    }
}

#[test]
fn todos_os_formatos_geram_arquivo_valido() {
    let ferr = ferramentas();
    let p = pasta("formatos");
    let arq = p.join("estereo.wav");
    ffmpeg(&["-f", "lavfi", "-i", "sine=f=440:d=2:r=48000", "-ac", "2", "-c:a", "pcm_s24le", arq.to_str().unwrap()]);
    for f in FORMATOS {
        if f.id == Formato::Mxf && ferr.raw2bmx.is_none() {
            eprintln!("(sem raw2bmx: pulando MXF)");
            continue;
        }
        let destino = p.join(f.nome.replace(' ', "_"));
        let r = converter(&ferr, &arq, &Opcoes { destino: Some(destino), ..op(f.id) });
        let esperado = if f.id == Formato::Mxf { 2 } else { 1 };
        assert_eq!(r.arquivos.len(), esperado, "{}", f.nome);
        for a in &r.arquivos {
            assert!(a.ends_with(&format!(".{}", f.extensao)), "{a}");
            let i = motor::inspecionar(&ferr, Path::new(a), F23).unwrap_or_else(|e| panic!("{}: {e:#}", f.nome));
            assert!((i.duracao_s - 2.0).abs() < 0.15, "{}: duração {}", f.nome, i.duracao_s);
        }
    }
}

fn info_mxf(arq: &str) -> String {
    let s = Command::new("mxf2raw").args(["--info"]).arg(arq).output().expect("mxf2raw");
    String::from_utf8_lossy(&s.stdout).into_owned()
}

#[test]
fn mxf_avid_com_timecode_alinhado() {
    let ferr = ferramentas();
    if ferr.raw2bmx.is_none() {
        eprintln!("(sem raw2bmx: pulando)");
        return;
    }
    let p = pasta("mxf");
    let arq = mov_51(&p);
    // juntos: um clipe com 6 faixas
    let r = converter(&ferr, &arq, &op(Formato::Mxf));
    assert_eq!(r.arquivos.len(), 6);
    assert!(r.arquivos[3].ends_with("EP18_CENA01_LFE.mxf"));
    let info = info_mxf(&r.arquivos[0]);
    assert!(info.contains("01:02:14:08"), "{info}");
    // separados: seis clipes
    let r = converter(&ferr, &arq, &Opcoes { separar: true, destino: Some(p.join("monos")), ..op(Formato::Mxf) });
    assert_eq!(r.arquivos.len(), 6);
    assert!(info_mxf(&r.arquivos[5]).contains("EP18_CENA01_Rs"));

    // WAV de 44,1 kHz com início no meio de um quadro: vira 48 kHz e começa no quadro anterior
    let w = p.join("meio.wav");
    let q = F23.tc_para_quadros("10:00:00:00").unwrap();
    let tr = q * 1839 + 500; // 44,1 kHz: 1839,3 amostras por quadro → meio do quadro
    ffmpeg(&["-f", "lavfi", "-i", "sine=f=440:d=1:r=44100", "-c:a", "pcm_s16le", "-write_bext", "1", "-metadata", &format!("time_reference={tr}"), w.to_str().unwrap()]);
    let r = converter(&ferr, &w, &Opcoes { bits: 16, ..op(Formato::Mxf) });
    assert_eq!(r.arquivos.len(), 1);
    let info = info_mxf(&r.arquivos[0]);
    let ins = motor::inspecionar(&ferr, &w, F23).unwrap();
    let tc_esperado = F23.quadros_para_tc(F23.segundos_para_quadros(ins.inicio.unwrap().segundos).floor() as u64);
    assert!(info.contains(&tc_esperado), "esperado {tc_esperado}\n{info}");
    assert!(r.avisos.iter().any(|a| a.contains("48 kHz")), "{:?}", r.avisos);
}
