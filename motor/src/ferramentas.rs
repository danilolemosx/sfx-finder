//! Programas externos que o motor usa: ffmpeg, ffprobe e raw2bmx (MXF do Avid).
//! No app eles vão embutidos ao lado do executável; nos testes e na linha de comando vêm do PATH.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::formatos::Encoders;

#[derive(Debug, Clone)]
pub struct Ferramentas {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub raw2bmx: Option<PathBuf>,
    pub encoders: Encoders,
}

fn nome_exe(nome: &str) -> String {
    if cfg!(windows) { format!("{nome}.exe") } else { nome.to_string() }
}

fn no_path(nome: &str) -> Option<PathBuf> {
    let exe = nome_exe(nome);
    std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join(&exe)).find(|c| c.is_file()))
}

/// Procura `nome` na pasta indicada, depois na variável SFXFINDER_FERRAMENTAS e por fim no PATH.
fn achar(nome: &str, pasta: Option<&Path>) -> Option<PathBuf> {
    let exe = nome_exe(nome);
    pasta
        .map(|p| p.join(&exe))
        .filter(|c| c.is_file())
        .or_else(|| std::env::var_os("SFXFINDER_FERRAMENTAS").map(|d| PathBuf::from(d).join(&exe)).filter(|c| c.is_file()))
        .or_else(|| no_path(nome))
}

/// Comando sem janela de console no Windows.
pub fn comando(programa: &Path) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(programa);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

impl Ferramentas {
    /// `pasta`: onde procurar primeiro (no app, a pasta do executável).
    pub fn localizar(pasta: Option<&Path>) -> Result<Self> {
        let Some(ffmpeg) = achar("ffmpeg", pasta) else { bail!("ffmpeg não encontrado") };
        let Some(ffprobe) = achar("ffprobe", pasta) else { bail!("ffprobe não encontrado") };
        let raw2bmx = achar("raw2bmx", pasta);
        let saida = comando(&ffmpeg).args(["-hide_banner", "-encoders"]).output().context("não foi possível rodar o ffmpeg")?;
        let lista = String::from_utf8_lossy(&saida.stdout);
        let tem = |n: &str| lista.lines().any(|l| l.split_whitespace().nth(1) == Some(n));
        let encoders = Encoders { libvorbis: tem("libvorbis"), libopus: tem("libopus") };
        Ok(Ferramentas { ffmpeg, ffprobe, raw2bmx, encoders })
    }
}
