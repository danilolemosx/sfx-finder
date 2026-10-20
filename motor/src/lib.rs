//! Motor do SFX Finder. Toda a lógica fica aqui, sem nada de interface, para poder ser testada sozinha
//! (`cargo test -p motor`) e usada pela linha de comando (`motor-cli`):
//! - `indice`: lê o banco de sons uma vez, guarda num SQLite e busca (com o gosto 👍/👎 e os sons já usados);
//! - `dicionario`: português ↔ inglês, para buscar em português e renomear;
//! - `ia`: a IA local (Qwen3.5-2B no llama.cpp) que entende a busca e dá nomes naturais em português;
//! - `nomes`: nome novo (traduzido e/ou encurtado);
//! - `envio`: copiar ou converter para a pasta temática, ORIGEM.txt, forma de onda e prévia;
//! - `conversao`, `inspecao`, `formatos`, `timecode`, `wav`: a conversão do Destila (ffmpeg, ffprobe e raw2bmx).

pub mod conversao;
pub mod dicionario;
pub mod envio;
pub mod ferramentas;
pub mod formatos;
pub mod ia;
pub mod indice;
pub mod inspecao;
pub mod nomes;
pub mod timecode;
pub mod wav;

pub use conversao::{converter, n_saidas, Opcoes, Resultado};
pub use dicionario::Dicionario;
pub use ferramentas::Ferramentas;
pub use formatos::{Formato, InfoFormato, FORMATOS};
pub use indice::{Consulta, Indice};
pub use inspecao::{inspecionar, Inspecao};
pub use timecode::{Fps, VELOCIDADES};
