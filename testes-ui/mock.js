// Tauri falso para testar a interface no navegador, sem o app.
// Cenas pela URL: ?cena=principal | primeira | lendo | sem-destino | vazio | convertendo | ordem | ia-pensando
"use strict";
(function () {
  const cena = new URLSearchParams(location.search).get("cena") || "principal";
  const ESTADO = window.__ESTADO__;
  const erros = [];
  window.__ERROS__ = erros;
  addEventListener("error", (e) => erros.push(String(e.message)));
  addEventListener("unhandledrejection", (e) => erros.push(String(e.reason)));

  const B = "/Volumes/INSIDE_10TB/03_BIBLIOTECA DE EDIÇÃO/SFX/01_BIBLIOTECAS";
  const som = (bib, pasta, nome, ext, dur, sr, bits, ch, voto = 0, usado = null, kbps = null) =>
    ({ caminho: `${B}/${bib}/${pasta}/${nome}.${ext}`, nome, ext, dur, sr, ch, bits, kbps, grupo: "01_BIBLIOTECAS", biblioteca: bib, pasta, meta: "", voto, usado });
  const SONS = [
    som("Sony Pictures Sound Effects Series", "Home & Office", "Door Metal Slam 02", "wav", 2.4, 48000, 24, 2, 1),
    som("Sony Pictures Sound Effects Series", "Home & Office", "Door Close Slam", "wav", 1.8, 48000, 24, 2, 1, "/Volumes/B_03 JOBS/SFX_EP12/PORTAS"),
    som("Warner Bros. Sound Effects Library", "Doors & Windows", "Door Slam Hard Reverb", "aiff", 3.6, 48000, 24, 1, 1),
    som("Sound Ideas", "Series 6000 General › CD 12", "Heavy Wooden Door Slam Interior", "wav", 2.9, 44100, 16, 2),
    som("SoundEffectsFactory", "", "Car Door Open And Slam", "mp3", 3.1, 44100, null, 2, 0, null, 320),
    som("BBC Sound Effects (completo MP3)", "Machines", "07027096 Church Door Interior, Slams, bolts lock door", "mp3", 4.6, 44100, null, 2, 0, null, 192),
    som("Sony Pictures Sound Effects Series", "Home & Office", "Door Screen Open Slam", "wav", 2.2, 48000, 24, 2),
    som("Valentino Production", "Vol. 4", "Prison Cell Door Slam", "flac", 4.1, 48000, 24, 2),
    som("Sony Pictures Sound Effects Series", "Home & Office", "Laundry Dryer Door Slam", "wav", 1.2, 48000, 24, 1),
    som("BBC Sound Effects (completo MP3)", "Transport", "07045298 Fire engine, door slammed", "mp3", 2.7, 44100, null, 2, -1, null, 192),
  ];
  const BIBS = [
    ["00_INBOX", "", 1], ["01_BIBLIOTECAS", "1001 Sound Effects", 1002], ["01_BIBLIOTECAS", "AKAI Sound Effects", 640],
    ["01_BIBLIOTECAS", "BBC Sound Effects (completo MP3)", 33066], ["01_BIBLIOTECAS", "Sony Pictures Sound Effects Series", 2310],
    ["01_BIBLIOTECAS", "Sound Ideas", 6120], ["01_BIBLIOTECAS", "Valentino Production", 4033], ["01_BIBLIOTECAS", "Warner Bros. Sound Effects Library", 1200],
    ["02_SELECAO_PESSOAL", "SELECT_BUBU", 31], ["02_SELECAO_PESSOAL", "sFX_mecanico", 12],
  ].map(([grupo, biblioteca, n]) => ({ grupo, biblioteca, n }));

  function picos(seed) {
    const v = [];
    let x = seed;
    for (let i = 0; i < 200; i++) {
      x = (x * 9301 + 49297) % 233280;
      const env = Math.exp(-i / 56) * (i < 4 ? (i + 1) / 4 : 1);
      v.push(Math.round(Math.min(1, (env * 0.9 + 0.1) * (0.55 + (x / 233280) * 0.6)) * 255));
    }
    return v;
  }

  class Channel { constructor() { this.onmessage = null; } }
  let iaFeita = false;
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

  async function invoke(cmd, args) {
    switch (cmd) {
      case "estado":
        if (cena === "primeira" || cena === "lendo") return { ...ESTADO, banco: null, bancoExiste: false, total: 0, atualizadoEm: null };
        return ESTADO;
      case "bibliotecas": return cena === "primeira" || cena === "lendo" ? [] : BIBS;
      case "buscar": {
        const c = args.consulta;
        if (!c.texto && !c.grupo && !c.lista) return { total: 0, sons: [], bibliotecas: [], traducoes: [] };
        if (cena === "vazio") return { total: 0, sons: [], bibliotecas: [], traducoes: [] };
        const conta = {};
        SONS.forEach((s) => { conta[s.biblioteca] = (conta[s.biblioteca] || 0) + 1; });
        return {
          total: 214, sons: SONS, ia: !!(args.usarIa && iaFeita),
          traducoes: args.usarIa && iaFeita ? ["door slam", "door bang", "door knock", "door close hard", "door shut"] : c.texto.includes("porta") ? ["door slam", "door knock"] : [],
          bibliotecas: [["Sony Pictures Sound Effects Series", 48], ["Sound Ideas", 41], ["BBC Sound Effects (completo MP3)", 37], ["Warner Bros. Sound Effects Library", 29], ["Valentino Production", 22], ["AKAI Sound Effects", 14], ["SoundEffectsFactory", 9], ["00_INBOX", 1]]
            .map(([b, n]) => ({ grupo: b === "00_INBOX" ? "00_INBOX" : "01_BIBLIOTECAS", biblioteca: b === "00_INBOX" ? "" : b, n })),
        };
      }
      case "ondas":
        for (const c of args.caminhos) {
          await sleep(5);
          args.canal.onmessage && args.canal.onmessage({ caminho: c, picos: picos(c.length * 37 + 11) });
        }
        return null;
      case "sugerir_nomes": {
        if (args.usarIa) await sleep(cena === "ia-pensando" ? 60000 : 30);
        const tab = { "Door Metal Slam 02": "Porta de Metal Batendo", "Door Close Slam": "Porta Fecha Batendo", "Car Door Open And Slam": "Carro Porta Abre", "07027096 Church Door Interior, Slams, bolts lock door": "Igreja Porta Interno" };
        return args.nomes.map((n) => (args.traduzir ? tab[n] || n : n));
      }
      case "votar": return ESTADO.ranking;
      case "ia_termos": await sleep(cena === "ia-pensando" ? 60000 : 50); iaFeita = true; return true;
      case "pasta_existe": return cena !== "sem-destino";
      case "subpastas": return ["ARMAS", "LUTA", "MULTIDAO", "PORTAS", "VENTO"];
      case "criar_subpasta": return args.nome;
      case "indexar":
        args.progresso.onmessage({ vistos: 62000, total: 62000, lidos: 23712, aLer: 62351 });
        return new Promise(() => {});
      case "enviar": {
        const p = args.progresso;
        p.onmessage({ item: 1, p: 0.45 });
        return new Promise(() => {});
      }
      case "icone_arraste": return "data:image/png;base64,";
      default: return null;
    }
  }

  window.__TAURI__ = {
    core: { invoke, Channel, convertFileSrc: (p) => "about:blank#" + encodeURIComponent(p) },
    dialog: { open: async () => null, message: async () => {}, ask: async () => true },
    opener: { revealItemInDir: async () => {} },
  };

  // preferências de cada cena
  const pref = { destino: "/Volumes/B_03 JOBS/SFX_EP18", tema: "PORTAS", acao: "converter", formato: "mxf", taxa: 48000, bits: 24, traduzir: true, encurtar: true, origem: true };
  if (cena === "sem-destino") pref.destino = null;
  if (cena === "ordem") pref.ordem = { coluna: "duracao", desc: true };
  try { localStorage.setItem("sfxfinder", JSON.stringify(pref)); } catch (_) { /* sem storage */ }

  // depois que o app abre: digita a busca, enche a cesta, abre a primeira vez…
  addEventListener("load", () => setTimeout(async () => {
    const q = document.querySelector("#q");
    if (cena === "lendo") document.querySelector("#boas-pasta").click();
    if (cena === "primeira" || cena === "lendo") {
      if (cena === "lendo") window.__TAURI__.dialog.open = async () => "/Volumes/INSIDE_10TB/03_BIBLIOTECA DE EDIÇÃO/SFX";
      if (cena === "lendo") document.querySelector("#boas-pasta").click();
      return;
    }
    q.value = cena === "vazio" ? "xyzw" : "porta batendo";
    q.dispatchEvent(new Event("input"));
    await sleep(400);
    if (cena === "vazio") return;
    q.blur();
    for (const i of [0, 1, 4, 5]) document.querySelector(`.lin[data-i="${i}"] [data-acao="cesta"]`)?.click();
    await sleep(100);
    const it = document.querySelector('.item[data-k="1"] input');
    if (it) { it.value = "Porta Batendo Forte"; it.dispatchEvent(new Event("input", { bubbles: true })); }
    if (cena === "convertendo") document.querySelector("#ir").click();
    document.body.dataset.erros = erros.join(" | ");
  }, 300));
})();
