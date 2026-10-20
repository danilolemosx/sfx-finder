"use strict";

const T = window.__TAURI__;
const { invoke, Channel, convertFileSrc } = T.core;
const dialogo = T.dialog;

const $ = (s) => document.querySelector(s);
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
const milhar = (n) => Number(n || 0).toLocaleString("pt-BR");
const plural = (n, um, varios) => `${milhar(n)} ${n === 1 ? um : varios}`;
const virgula = (v, casas = 1) => v.toFixed(casas).replace(".", ",");
const SEM_PERDA = ["wav", "bwf", "aif", "aiff", "flac", "caf", "w64"];
function dur(s) {
  if (s == null) return "—";
  if (s < 60) return `${virgula(s)} s`;
  const m = Math.floor(s / 60), ss = Math.round(s % 60);
  return `${m}:${String(ss).padStart(2, "0")}`;
}
function relogio(s) {
  const m = Math.floor(s / 60), r = s - m * 60;
  return `${m}:${r < 10 ? "0" : ""}${virgula(r)}`;
}
const nomeDaPasta = (p) => String(p || "").split(/[\\/]/).filter(Boolean).pop() || p;
const paiDaPasta = (p) => String(p || "").replace(/[\\/][^\\/]*[\\/]?$/, "");
const juntar = (a, b) => (a.endsWith("/") || a.endsWith("\\") ? a + b : a + (a.includes("\\") && !a.includes("/") ? "\\" : "/") + b);
/** "01_BIBLIOTECAS" → "BIBLIOTECAS" */
const rotuloGrupo = (g) => (g ? g.replace(/^\d+[\s_-]*/, "").replace(/_/g, " ") || g : "Soltos");

/** Espera no máximo `ms`; se o disco não responder, segue com `padrao` (a pasta pode estar num HD dormindo). */
const comTempo = (p, ms, padrao) => Promise.race([p.catch(() => padrao), new Promise((ok) => setTimeout(() => ok(padrao), ms))]);

// ---------- preferências (lembradas entre aberturas) ----------
const PADRAO = {
  destino: null, tema: null, acao: "converter", formato: "wav", taxa: 48000, bits: 24, kbps: {}, fps: { num: 25, den: 1, drop: false },
  traduzir: true, encurtar: true, origem: true, repetir: false, ordem: null, ia: true,
  filtros: { durMax: null, durMin: null, semPerda: false, canais: null, gosto: "todos" },
};
let pref = structuredClone(PADRAO);
try {
  const lido = JSON.parse(localStorage.getItem("sfxfinder") || "{}");
  pref = { ...pref, ...lido, filtros: { ...PADRAO.filtros, ...(lido.filtros || {}) } };
} catch (_) { /* sem preferências */ }
function salvar() { try { localStorage.setItem("sfxfinder", JSON.stringify(pref)); } catch (_) { /* ignora */ } }

// ---------- estado ----------
let est = { formatos: [], velocidades: [], ranking: { gostei: 0, naoGostei: 0, usados: 0 } };
let todasBibs = [];          // bibliotecas do banco inteiro (para navegar sem busca)
let resp = { total: 0, sons: [], bibliotecas: [], traducoes: [] };
let filtroBib = null;        // { grupo, biblioteca } ou null
let lista = null;            // "gostei" | "naoGostei" | "usados" | null
let sel = -1;                // som selecionado
const ondas = new Map();     // caminho → picos (0..255)
const cesta = [];            // { caminho, original, ext, dur, biblioteca, nome, editado, trecho, estado, p, msg }
let temas = [];
let enviando = false;
let indexando = false;
let iconeArraste = null;

// ---------- busca ----------
let pedido = 0;
function consulta() {
  const f = pref.filtros;
  return {
    texto: $("#q").value.trim(),
    durMin: f.durMin, durMax: f.durMax, soSemPerda: f.semPerda, canais: f.canais, gosto: f.gosto,
    grupo: filtroBib ? filtroBib.grupo : null, biblioteca: filtroBib ? filtroBib.biblioteca : null,
    lista, ordem: pref.ordem, limite: 500,
  };
}

const usarIa = () => !!pref.ia && est.ia === "pronta";
let iaPensando = false;
async function buscar(depoisDaIa = false) {
  const meu = ++pedido;
  const c = consulta();
  let r;
  try { r = await invoke("buscar", { consulta: c, usarIa: usarIa() }); } catch (e) { aviso(`Erro na busca: ${e}`); return; }
  if (meu !== pedido) return;
  // a IA ainda não conhece esta busca: mostra o resultado do dicionário e pede a ela em seguida
  iaPensando = !depoisDaIa && usarIa() && !r.ia && /[\p{L}\p{N}]/u.test(c.texto.replace(/(^|\s)-\S+/g, ""));
  if (iaPensando) {
    invoke("ia_termos", { texto: c.texto }).then((ok) => {
      if (meu !== pedido) return;
      iaPensando = false;
      if (ok) buscar(true); else renderDica();
    }).catch(() => { if (meu === pedido) { iaPensando = false; renderDica(); } });
  }
  const antes = sel >= 0 ? resp.sons[sel]?.caminho : null;
  resp = r;
  sel = antes ? resp.sons.findIndex((s) => s.caminho === antes) : -1;
  if (sel < 0 && resp.sons.length) sel = 0;
  renderDica();
  renderBibs();
  renderLinhas();
  renderPlayerInfo();
  pedirOndas();
}
let tBusca = 0;
function buscarDepois() { clearTimeout(tBusca); tBusca = setTimeout(buscar, 220); }

function renderDica() {
  const d = resp.traducoes || [];
  const selo = `<span class="selo-ia" title="IA local (Qwen), sem internet">✦ IA</span>`;
  if (iaPensando) $("#dica").innerHTML = `${selo} pensando…${d.length ? ` · <b>${d.map(esc).join(" · ")}</b>` : ""}`;
  else if (resp.ia) $("#dica").innerHTML = `${selo} buscando também <b>${d.map(esc).join(" · ")}</b>`;
  else $("#dica").innerHTML = d.length ? `→ buscando também <b>${d.map(esc).join(" · ")}</b>` : "";
  const NOMES = { nome: "nome", duracao: "duração", formato: "formato", gosto: "gosto" };
  const o = pref.ordem;
  const ordem = o ? `ordem: <i>${NOMES[o.coluna]} ${o.desc ? "decrescente" : "crescente"}</i> · <button class="link" id="ordem-normal">normal</button>` : `ordem: <i>gostei primeiro, depois qualidade</i>`;
  const temBusca = consulta().texto || filtroBib || lista;
  $("#cont").innerHTML = temBusca ? `<b class="tab">${milhar(resp.total)}</b> ${resp.total === 1 ? "som encontrado" : "sons encontrados"}${resp.total > resp.sons.length ? ` (mostrando ${milhar(resp.sons.length)})` : ""} · ${ordem}` : "";
}

// ---------- ordem pelas colunas: 1º clique crescente, 2º decrescente, 3º volta ao normal ----------
function renderCab() {
  document.querySelectorAll("#cab [data-col]").forEach((b) => {
    const on = pref.ordem && pref.ordem.coluna === b.dataset.col;
    b.classList.toggle("on", !!on);
    b.classList.toggle("desc", !!(on && pref.ordem.desc));
    b.title = "Ordenar por esta coluna";
  });
}
$("#cab").addEventListener("click", (e) => {
  const b = e.target.closest("[data-col]");
  if (!b) return;
  const c = b.dataset.col, o = pref.ordem;
  pref.ordem = !o || o.coluna !== c ? { coluna: c, desc: false } : !o.desc ? { coluna: c, desc: true } : null;
  salvar(); renderCab(); buscar();
});
$("#cont").addEventListener("click", (e) => {
  if (e.target.id !== "ordem-normal") return;
  pref.ordem = null; salvar(); renderCab(); buscar();
});

// ---------- filtros ----------
const OP_FILTROS = {
  dur: { nome: "Duração", ops: [["todas", null, null], ["até 2 s", null, 2], ["até 5 s", null, 5], ["até 10 s", null, 10], ["até 30 s", null, 30], ["de 30 s a 2 min", 30, 120], ["mais de 2 min", 120, null]] },
  qual: { nome: "Qualidade", ops: [["todas", false], ["WAV · AIFF · FLAC", true]] },
  canais: { nome: "Canais", ops: [["todos", null], ["mono", 1], ["estéreo", 2], ["mais de 2", 3]] },
  gosto: { nome: "Gosto", ops: [["todos", "todos"], ["só os que gostei", "soGostei"], ["sem os que não gostei", "semRuins"]] },
};
function opAtual(k) {
  const f = pref.filtros, o = OP_FILTROS[k].ops;
  if (k === "dur") return o.findIndex((x) => x[1] === f.durMin && x[2] === f.durMax);
  if (k === "qual") return o.findIndex((x) => x[1] === f.semPerda);
  if (k === "canais") return o.findIndex((x) => x[1] === f.canais);
  return o.findIndex((x) => x[1] === f.gosto);
}
function aplicarFiltro(k, i) {
  const f = pref.filtros, x = OP_FILTROS[k].ops[i];
  if (k === "dur") { f.durMin = x[1]; f.durMax = x[2]; }
  if (k === "qual") f.semPerda = x[1];
  if (k === "canais") f.canais = x[1];
  if (k === "gosto") f.gosto = x[1];
  salvar(); renderFiltros(); buscar();
}
function renderFiltros() {
  document.querySelectorAll(".chip").forEach((b) => {
    const k = b.dataset.filtro, i = Math.max(0, opAtual(k));
    b.innerHTML = `${OP_FILTROS[k].nome} <b>${esc(OP_FILTROS[k].ops[i][0])}</b>`;
    b.classList.toggle("on", i > 0);
  });
}

/** Menu flutuante abaixo de `alvo`: itens [rótulo, ativo, ação]. */
function menu(alvo, itens) {
  const m = $("#menu");
  m.innerHTML = itens.map(([r, on], i) => `<button data-i="${i}" class="${on ? "on" : ""}">${esc(r)}</button>`).join("");
  m.classList.remove("oculto");
  const r = alvo.getBoundingClientRect();
  m.style.left = `${Math.min(r.left, innerWidth - m.offsetWidth - 10)}px`;
  m.style.top = `${r.bottom + 6 + m.offsetHeight > innerHeight ? r.top - m.offsetHeight - 6 : r.bottom + 6}px`;
  m.onclick = (e) => {
    const b = e.target.closest("button[data-i]");
    if (!b) return;
    fecharMenu();
    itens[+b.dataset.i][2]();
  };
}
function fecharMenu() { $("#menu").classList.add("oculto"); }

// ---------- bibliotecas (coluna da esquerda) ----------
function renderBibs() {
  const temBusca = !!consulta().texto;
  const fonte = temBusca ? resp.bibliotecas : todasBibs;
  const grupos = new Map();
  for (const b of fonte) {
    if (!grupos.has(b.grupo)) grupos.set(b.grupo, []);
    grupos.get(b.grupo).push(b);
  }
  const nomes = [...grupos.keys()].sort((a, b) => a.localeCompare(b));
  const total = fonte.reduce((s, b) => s + b.n, 0);
  let h = `<button class="bib ${!filtroBib && !lista ? "on" : ""}" data-bib=""><span>Todas</span><span class="n tab">${milhar(total)}</span></button>`;
  const r = est.ranking;
  for (const g of nomes) {
    const itens = grupos.get(g).slice().sort((a, b) => (temBusca ? b.n - a.n : 0) || a.biblioteca.localeCompare(b.biblioteca));
    h += `<hr><h3 class="rot">${esc(rotuloGrupo(g))}</h3>`;
    h += itens.map((b) => {
      const on = filtroBib && filtroBib.grupo === b.grupo && filtroBib.biblioteca === b.biblioteca;
      const nome = b.biblioteca || rotuloGrupo(b.grupo);
      return `<button class="bib ${on ? "on" : ""}" data-g="${esc(b.grupo)}" data-b="${esc(b.biblioteca)}" title="${esc(nome)}"><span>${esc(nome)}</span><span class="n tab">${milhar(b.n)}</span></button>`;
    }).join("");
  }
  if (!fonte.length && temBusca) h += `<p class="vazio-bib">Nenhuma biblioteca com esse som.</p>`;
  h += `<hr><h3 class="rot">Meu ranking</h3>
    <button class="bib ${lista === "gostei" ? "on" : ""}" data-lista="gostei"><span><span class="ic up"></span> Gostei</span><span class="n tab">${milhar(r.gostei)}</span></button>
    <button class="bib ${lista === "naoGostei" ? "on" : ""}" data-lista="naoGostei"><span><span class="ic dn"></span> Não gostei</span><span class="n tab">${milhar(r.naoGostei)}</span></button>
    <button class="bib ${lista === "usados" ? "on" : ""}" data-lista="usados"><span><span class="usado"></span> Já usados</span><span class="n tab">${milhar(r.usados)}</span></button>`;
  $("#bibs").innerHTML = h;
}

$("#bibs").addEventListener("click", (e) => {
  const b = e.target.closest(".bib");
  if (!b) return;
  if (b.dataset.lista) {
    lista = lista === b.dataset.lista ? null : b.dataset.lista;
    filtroBib = null;
  } else if (b.dataset.bib === "") {
    filtroBib = null; lista = null;
  } else {
    const novo = { grupo: b.dataset.g, biblioteca: b.dataset.b };
    filtroBib = filtroBib && filtroBib.grupo === novo.grupo && filtroBib.biblioteca === novo.biblioteca ? null : novo;
    lista = null;
  }
  buscar();
});

// ---------- resultados ----------
function destacar(nome) {
  const termos = [...consulta().texto.split(/\s+/).filter((t) => t && !t.startsWith("-")), ...(resp.traducoes || []).flatMap((t) => t.split(" "))]
    .map((t) => t.replace(/[^\p{L}\p{N}]/gu, "")).filter((t) => t.length >= 2);
  let h = esc(nome);
  if (!termos.length) return h;
  const sem = (s) => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();
  const alvo = sem(nome);
  const marcas = [];
  for (const t of new Set(termos.map(sem))) {
    const re = new RegExp(`(^|[^\\p{L}\\p{N}])(${t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}[\\p{L}]*)`, "gu");
    let m;
    while ((m = re.exec(alvo))) marcas.push([m.index + m[1].length, m.index + m[0].length]);
  }
  if (!marcas.length) return h;
  marcas.sort((a, b) => a[0] - b[0]);
  let out = "", i = 0;
  for (const [a, b] of marcas) {
    if (a < i) continue;
    out += esc(nome.slice(i, a)) + "<mark>" + esc(nome.slice(a, b)) + "</mark>";
    i = b;
  }
  return out + esc(nome.slice(i));
}

function infoFormato(s) {
  const k = s.sr ? (s.sr % 1000 ? virgula(s.sr / 1000) : s.sr / 1000) + "k" : "";
  const meio = SEM_PERDA.includes(s.ext) ? (s.bits || "") : s.kbps ? s.kbps : "";
  const ch = s.ch === 1 ? "mono" : s.ch === 2 ? "st" : s.ch ? `${s.ch} ch` : "";
  return [k, meio, ch].filter(Boolean).join(" · ");
}
const naCesta = (c) => cesta.some((i) => i.caminho === c);

function linhaHtml(s, i) {
  const usado = s.usado ? `<span class="usado"></span><b>usado em ${esc(nomeDaPasta(paiDaPasta(s.usado)) ? nomeDaPasta(paiDaPasta(s.usado)) + "/" : "")}${esc(nomeDaPasta(s.usado))}</b> · ` : "";
  const origem = [s.biblioteca || rotuloGrupo(s.grupo), s.pasta].filter(Boolean).join(" › ");
  const tocandoEste = i === sel && !audio.paused;
  return `<div class="lin ${i === sel ? "sel" : ""} ${tocandoEste ? "tocando" : ""} ${s.voto < 0 ? "ruim" : ""}" data-i="${i}">
    <button class="play" data-acao="tocar">${tocandoEste ? "❚❚" : "▶"}</button>
    <div style="min-width:0"><div class="nome" title="${esc(s.nome)}${s.meta ? "\n" + esc(s.meta) : ""}">${destacar(s.nome)}</div><div class="cam" title="${esc(s.caminho)}">${usado}${esc(origem)}</div></div>
    <canvas data-onda="${i}"></canvas>
    <div class="dur tab">${dur(s.dur)}</div>
    <div class="fmt"><b class="${SEM_PERDA.includes(s.ext) ? "" : "perda"}">${esc(s.ext)}</b><br>${esc(infoFormato(s))}</div>
    <div class="voto"><button data-acao="up" title="Gostei"><span class="ic up ${s.voto > 0 ? "on" : ""}"></span></button><button data-acao="dn" title="Não gostei"><span class="ic dn ${s.voto < 0 ? "on" : ""}"></span></button></div>
    <button class="add ${naCesta(s.caminho) ? "ok" : ""}" data-acao="cesta" title="Pôr na cesta (enter)">${naCesta(s.caminho) ? "✓" : "+"}</button>
  </div>`;
}

function renderLinhas() {
  const el = $("#linhas");
  if (!resp.sons.length) {
    const c = consulta();
    if (indexando && !est.total) el.innerHTML = `<div class="vazio"><b>LENDO O BANCO…</b>Os sons aparecem aqui assim que forem lidos.</div>`;
    else if (c.texto || filtroBib || lista) el.innerHTML = `<div class="vazio"><b>NADA ENCONTRADO</b>Tente outras palavras, em português ou em inglês, ou afrouxe os filtros.</div>`;
    else el.innerHTML = `<div class="vazio"><b>O QUE VOCÊ PROCURA?</b>Digite no alto: <i>porta batendo</i>, <i>chuva forte</i>, <i>whoosh</i>…<br>Ou escolha uma biblioteca à esquerda para navegar.</div>`;
    return;
  }
  el.innerHTML = resp.sons.map(linhaHtml).join("") + (resp.total > resp.sons.length ? `<div class="mais">Mostrando os ${milhar(resp.sons.length)} primeiros de ${milhar(resp.total)}. Refine a busca para ver outros.</div>` : "");
  el.scrollTop = 0;
  el.querySelectorAll("canvas[data-onda]").forEach((cv) => observador.observe(cv));
  rolarParaSel();
}

function atualizarLinha(i) {
  const velho = $(`.lin[data-i="${i}"]`);
  if (!velho || !resp.sons[i]) return;
  const tmp = document.createElement("div");
  tmp.innerHTML = linhaHtml(resp.sons[i], i);
  const novo = tmp.firstElementChild;
  velho.replaceWith(novo);
  const cv = novo.querySelector("canvas");
  desenharMini(cv, i);
  observador.observe(cv);
}

function rolarParaSel() {
  const l = $(`.lin[data-i="${sel}"]`);
  if (l) l.scrollIntoView({ block: "nearest" });
}

// formas de onda: pede só as das linhas que aparecem
const visiveis = new Set();
const observador = new IntersectionObserver((ents) => {
  for (const e of ents) {
    const i = +e.target.dataset.onda;
    if (e.isIntersecting) visiveis.add(i); else visiveis.delete(i);
  }
  pedirOndasDepois();
}, { root: $("#linhas"), rootMargin: "200px" });
let tOndas = 0;
function pedirOndasDepois() { clearTimeout(tOndas); tOndas = setTimeout(pedirOndas, 120); }
const pedidas = new Set();
function pedirOndas() {
  const faltam = [...visiveis].sort((a, b) => a - b).map((i) => resp.sons[i]?.caminho).filter((c) => c && !ondas.has(c) && !pedidas.has(c));
  for (const i of visiveis) desenharMini($(`canvas[data-onda="${i}"]`), i);
  if (!faltam.length) return;
  faltam.forEach((c) => pedidas.add(c));
  const canal = new Channel();
  canal.onmessage = (o) => {
    ondas.set(o.caminho, o.picos);
    pedidas.delete(o.caminho);
    resp.sons.forEach((s, i) => { if (s.caminho === o.caminho) desenharMini($(`canvas[data-onda="${i}"]`), i); });
    if (sel >= 0 && resp.sons[sel]?.caminho === o.caminho) desenharGrande();
  };
  invoke("ondas", { caminhos: faltam, canal }).catch(() => {}).finally(() => faltam.forEach((c) => pedidas.delete(c)));
}

const cor = (v) => getComputedStyle(document.documentElement).getPropertyValue(v).trim();
function prepararCanvas(cv) {
  const r = cv.getBoundingClientRect(), dpr = devicePixelRatio || 1;
  const w = Math.max(1, Math.round(r.width * dpr)), h = Math.max(1, Math.round(r.height * dpr));
  if (cv.width !== w || cv.height !== h) { cv.width = w; cv.height = h; }
  const g = cv.getContext("2d");
  g.clearRect(0, 0, w, h);
  return { g, w, h, dpr };
}
/** Barras verticais com os picos; `ate` (0..1) pinta em vermelho o que já tocou. */
function barras(cv, picos, n, ate, opacidade) {
  if (!cv) return;
  const { g, w, h, dpr } = prepararCanvas(cv);
  const passo = w / n, larg = Math.max(1, passo - 1 * dpr);
  for (let k = 0; k < n; k++) {
    const a = Math.floor((k * picos.length) / n), b = Math.max(a + 1, Math.floor(((k + 1) * picos.length) / n));
    let v = 0;
    for (let j = a; j < b; j++) v = Math.max(v, picos[j]);
    const alt = Math.max(2 * dpr, (v / 255) * h);
    g.globalAlpha = k / n < ate ? 1 : opacidade;
    g.fillStyle = k / n < ate ? cor("--destaque") : cor("--tinta");
    g.fillRect(k * passo, (h - alt) / 2, larg, alt);
  }
  g.globalAlpha = 1;
}
function desenharMini(cv, i) {
  const s = resp.sons[i];
  if (!cv || !s) return;
  const p = ondas.get(s.caminho);
  if (!p) {
    const { g, w, h } = prepararCanvas(cv);
    g.fillStyle = cor("--linha");
    g.fillRect(0, h / 2 - 1, w, 2);
    return;
  }
  const tocou = i === sel && audio.duration ? audio.currentTime / audio.duration : 0;
  barras(cv, p, 44, tocou, i === sel ? 0.9 : 0.55);
}

// ---------- player ----------
const audio = new Audio();
audio.preload = "auto";
let trecho = null;                  // [ini, fim] em segundos, no som selecionado
let tentouPrevia = false;
let atual = null;                   // caminho carregado no player
const somSel = () => (sel >= 0 ? resp.sons[sel] : null);

function carregar(s) {
  if (atual === s.caminho) return;
  atual = s.caminho;
  tentouPrevia = false;
  trecho = null;
  audio.src = convertFileSrc(s.caminho);
}
audio.addEventListener("error", async () => {
  // formato que a janela não toca (AIFF no Windows, WMA…): toca uma prévia em WAV
  const s = somSel();
  if (!s || tentouPrevia || atual !== s.caminho) return;
  tentouPrevia = true;
  try {
    const wav = await invoke("previa", { caminho: s.caminho });
    if (atual !== s.caminho) return;
    const tocando = deveTocar;
    audio.src = convertFileSrc(wav);
    if (tocando) audio.play().catch(() => {});
  } catch (e) { aviso(`Não deu para tocar este som: ${e}`); }
});
let deveTocar = false;
function tocar(i) {
  if (i != null && i !== sel) selecionar(i);
  const s = somSel();
  if (!s) return;
  carregar(s);
  if (!audio.paused) { audio.pause(); deveTocar = false; return; }
  deveTocar = true;
  if (trecho && (audio.currentTime < trecho[0] || audio.currentTime >= trecho[1] - 0.02)) audio.currentTime = trecho[0];
  else if (audio.ended || (audio.duration && audio.currentTime >= audio.duration - 0.02)) audio.currentTime = 0;
  audio.play().catch(() => {});
}
function parar() { audio.pause(); deveTocar = false; }
function selecionar(i, tocarJunto = false) {
  if (i < 0 || i >= resp.sons.length) return;
  const anterior = sel;
  const tocava = !audio.paused;
  sel = i;
  atualizarLinha(anterior);
  atualizarLinha(sel);
  rolarParaSel();
  const s = somSel();
  if (atual !== s.caminho) {
    audio.pause();
    carregar(s);
    if (tocava || tocarJunto) { deveTocar = true; audio.play().catch(() => {}); }
  }
  renderPlayerInfo();
}
["play", "pause", "ended"].forEach((ev) => audio.addEventListener(ev, () => { atualizarLinha(sel); renderPlayerInfo(); }));
audio.addEventListener("ended", () => {
  if (pref.repetir) { audio.currentTime = trecho ? trecho[0] : 0; audio.play().catch(() => {}); }
  else deveTocar = false;
});
audio.addEventListener("loadedmetadata", renderPlayerInfo);
function quadro() {
  if (!audio.paused && trecho && audio.currentTime >= trecho[1]) {
    if (pref.repetir) audio.currentTime = trecho[0];
    else { audio.pause(); audio.currentTime = trecho[0]; deveTocar = false; }
  }
  if (!audio.paused) {
    desenharGrande();
    desenharMini($(`canvas[data-onda="${sel}"]`), sel);
    renderTempo();
  }
  requestAnimationFrame(quadro);
}
requestAnimationFrame(quadro);

function duracaoAtual() {
  const s = somSel();
  return audio.duration && isFinite(audio.duration) && atual === s?.caminho ? audio.duration : s?.dur || 0;
}
function renderTempo() {
  const s = somSel();
  if (!s) { $("#p-tempo").textContent = ""; return; }
  const d = duracaoAtual();
  const t = atual === s.caminho ? audio.currentTime : 0;
  $("#p-tempo").textContent = `${relogio(t)} / ${relogio(d)}${trecho ? ` · trecho ${relogio(trecho[0])} – ${relogio(trecho[1])}` : ""}`;
}
function renderPlayerInfo() {
  const s = somSel();
  $("#p-nome").textContent = s ? s.nome : "—";
  $("#tocar").disabled = !s;
  $("#tocar").textContent = s && !audio.paused && atual === s.caminho ? "❚❚" : "▶";
  $("#revelar").disabled = !s;
  $("#repetir").classList.toggle("on", !!pref.repetir);
  renderTempo();
  desenharGrande();
}
function desenharGrande() {
  const s = somSel(), cv = $("#onda-grande");
  const d = duracaoAtual();
  const tr = $("#trecho"), cur = $("#cursor");
  if (!s) { prepararCanvas(cv); tr.classList.add("oculto"); cur.style.display = "none"; return; }
  const p = ondas.get(s.caminho);
  const t = atual === s.caminho ? audio.currentTime : 0;
  if (p) barras(cv, p, 150, d ? t / d : 0, 0.5);
  else { const { g, w, h } = prepararCanvas(cv); g.fillStyle = cor("--linha"); g.fillRect(0, h / 2 - 1, w, 2); }
  if (trecho && d) {
    tr.classList.remove("oculto");
    tr.style.left = `${(trecho[0] / d) * 100}%`;
    tr.style.width = `${((trecho[1] - trecho[0]) / d) * 100}%`;
  } else tr.classList.add("oculto");
  cur.style.display = d && atual === s.caminho && (t > 0 || !audio.paused) ? "block" : "none";
  cur.style.left = `${d ? (t / d) * 100 : 0}%`;
}

// clicar na onda: pula para o ponto; arrastar: marca um trecho
(() => {
  const el = $("#grande");
  let ini = null;
  const pos = (e) => {
    const r = el.getBoundingClientRect();
    return Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)) * duracaoAtual();
  };
  el.addEventListener("mousedown", (e) => {
    if (e.target.closest("#sem-trecho") || !somSel()) return;
    carregar(somSel());
    ini = { t: pos(e), x: e.clientX, arrastou: false };
  });
  addEventListener("mousemove", (e) => {
    if (!ini) return;
    if (Math.abs(e.clientX - ini.x) > 4) ini.arrastou = true;
    if (ini.arrastou) {
      const t = pos(e);
      trecho = [Math.min(ini.t, t), Math.max(ini.t, t)];
      desenharGrande(); renderTempo();
    }
  });
  addEventListener("mouseup", (e) => {
    if (!ini) return;
    const t = pos(e);
    if (ini.arrastou && trecho && trecho[1] - trecho[0] > 0.05) {
      audio.currentTime = trecho[0];
      atualizarTrechoNaCesta();
      if (!audio.paused || deveTocar) audio.play().catch(() => {});
    } else {
      if (!ini.arrastou) {
        if (trecho && (t < trecho[0] || t > trecho[1])) { trecho = null; atualizarTrechoNaCesta(); }
        audio.currentTime = t;
      }
      if (ini.arrastou) trecho = null;
    }
    ini = null;
    desenharGrande(); renderTempo();
  });
})();
$("#sem-trecho").addEventListener("click", () => { trecho = null; atualizarTrechoNaCesta(); desenharGrande(); renderTempo(); });
/** Se o som selecionado já está na cesta, o trecho vale para ele também. */
function atualizarTrechoNaCesta() {
  const s = somSel();
  const it = s && cesta.find((c) => c.caminho === s.caminho);
  if (it) { it.trecho = trecho ? [...trecho] : null; renderCesta(); }
}

$("#tocar").addEventListener("click", () => tocar());
$("#repetir").addEventListener("click", () => { pref.repetir = !pref.repetir; salvar(); renderPlayerInfo(); });
$("#revelar").addEventListener("click", () => { const s = somSel(); if (s) T.opener.revealItemInDir(s.caminho).catch(() => {}); });

// ---------- cliques e arrastar na lista ----------
$("#linhas").addEventListener("click", async (e) => {
  const l = e.target.closest(".lin");
  if (!l) return;
  const i = +l.dataset.i, s = resp.sons[i];
  const acao = e.target.closest("[data-acao]")?.dataset.acao;
  if (acao === "tocar") { if (i === sel) tocar(); else { selecionar(i, true); } return; }
  if (acao === "up" || acao === "dn") { await votar(i, acao === "up" ? 1 : -1); return; }
  if (acao === "cesta") { alternarCesta(s); return; }
  if (i !== sel) selecionar(i);
});
$("#linhas").addEventListener("dblclick", (e) => {
  const l = e.target.closest(".lin");
  if (l && !e.target.closest("button")) tocar(+l.dataset.i);
});
async function votar(i, v) {
  const s = resp.sons[i];
  const novo = s.voto === v ? 0 : v;
  try {
    est.ranking = await invoke("votar", { caminho: s.caminho, voto: novo });
    s.voto = novo;
    atualizarLinha(i);
    renderBibs();
  } catch (e) { aviso(`Erro: ${e}`); }
}
// arrastar um som para o Finder ou para um bin do Avid
(() => {
  let ini = null;
  $("#linhas").addEventListener("mousedown", (e) => {
    const l = e.target.closest(".lin");
    if (!l || e.button !== 0 || e.target.closest("button")) return;
    ini = { x: e.clientX, y: e.clientY, i: +l.dataset.i };
  });
  addEventListener("mousemove", async (e) => {
    if (!ini || Math.hypot(e.clientX - ini.x, e.clientY - ini.y) < 8) return;
    const s = resp.sons[ini.i];
    ini = null;
    if (!s) return;
    try {
      if (!iconeArraste) iconeArraste = await invoke("icone_arraste");
      await invoke("plugin:drag|start_drag", { item: [s.caminho], image: iconeArraste, onEvent: new Channel() });
    } catch (_) { /* arrastar não disponível */ }
  });
  addEventListener("mouseup", () => { ini = null; });
})();

// ---------- cesta ----------
async function sugerir(itens) {
  if (!itens.length) return;
  try {
    const ia = usarIa() && pref.traduzir;
    if (ia) { itens.forEach((it) => { if (!it.editado) it.pensando = true; }); renderCesta(); }
    const nomes = await invoke("sugerir_nomes", { nomes: itens.map((i) => i.original), descricoes: itens.map((i) => i.meta || ""), traduzir: !!pref.traduzir, encurtar: !!pref.encurtar, usarIa: ia });
    itens.forEach((it, k) => { if (!it.editado) it.nome = nomes[k]; it.pensando = false; });
  } catch (_) { itens.forEach((it) => { if (!it.editado) it.nome = it.original; it.pensando = false; }); }
}
async function alternarCesta(s) {
  const k = cesta.findIndex((c) => c.caminho === s.caminho);
  if (k >= 0) cesta.splice(k, 1);
  else {
    const it = { caminho: s.caminho, original: s.nome, meta: s.meta, ext: s.ext, dur: s.dur, biblioteca: s.biblioteca || rotuloGrupo(s.grupo), nome: s.nome, editado: false, trecho: s === somSel() && trecho ? [...trecho] : null };
    cesta.push(it);
    resp.sons.forEach((x, i) => { if (x.caminho === s.caminho) atualizarLinha(i); });
    renderCesta();
    await sugerir([it]);
  }
  resp.sons.forEach((x, i) => { if (x.caminho === s.caminho) atualizarLinha(i); });
  renderCesta();
}
function renderCesta() {
  $("#n-cesta").textContent = cesta.length;
  const el = $("#itens");
  if (!cesta.length) el.innerHTML = `<div class="vazio">A cesta está vazia. Use o <b style="display:inline;font-size:inherit">+</b> ou o enter para juntar os sons que vão para a pasta.</div>`;
  else {
    el.innerHTML = cesta.map((it, k) => {
      const d = it.trecho ? `${virgula(it.trecho[1] - it.trecho[0])} s ✂` : dur(it.dur);
      const de = it.estado === "erro" ? esc(it.msg) : `← ${esc(it.original)}.${esc(it.ext)} · ${esc(it.biblioteca)}`;
      const barra = it.estado === "indo" ? `<div class="barrinha"><i style="width:${Math.round((it.p || 0) * 100)}%"></i></div>` : "";
      return `<div class="item ${it.estado === "erro" ? "erro" : ""}" data-k="${k}"><div style="min-width:0"><input value="${esc(it.pensando ? "✦ pensando…" : it.nome)}" class="${it.editado ? "edit" : ""} ${it.pensando ? "pensando" : ""}" spellcheck="false" ${enviando ? "disabled" : ""}><div class="de" title="${esc(it.caminho)}">${de}</div></div><span class="d tab" title="${it.trecho ? "só o trecho marcado" : ""}">${d}</span><button class="x" title="Tirar da cesta">✕</button>${barra}</div>`;
    }).join("");
  }
  $("#esvaziar").disabled = enviando;
  renderIr();
}
$("#itens").addEventListener("input", (e) => {
  const inp = e.target.closest("input");
  if (!inp) return;
  const it = cesta[+inp.closest(".item").dataset.k];
  it.nome = inp.value;
  it.editado = true;
  it.pensando = false;
  inp.classList.add("edit");
  inp.classList.remove("pensando");
});
$("#itens").addEventListener("keydown", (e) => { if (e.key === "Enter") e.target.blur(); });
$("#itens").addEventListener("click", (e) => {
  if (!e.target.closest(".x") || enviando) return;
  const it = cesta.splice(+e.target.closest(".item").dataset.k, 1)[0];
  resp.sons.forEach((x, i) => { if (x.caminho === it.caminho) atualizarLinha(i); });
  renderCesta();
});
$("#esvaziar").addEventListener("click", () => {
  if (enviando) return;
  cesta.length = 0;
  renderLinhas();
  renderCesta();
});

// destino
function pastaFinal() { return pref.destino ? (pref.tema ? juntar(pref.destino, pref.tema) : pref.destino) : null; }
async function carregarTemas() {
  const lidas = pref.destino ? await comTempo(invoke("subpastas", { pasta: pref.destino }), 3000, null) : [];
  temas = lidas || [];
  // se o disco não respondeu, mantém a subpasta escolhida
  if (lidas && pref.tema && !temas.includes(pref.tema)) { pref.tema = null; salvar(); }
  if (!lidas && pref.tema) temas = [pref.tema];
  renderDestino();
}
function renderDestino() {
  const b = $("#destino");
  if (pref.destino) {
    b.className = "caixa";
    b.innerHTML = `<span>${esc(nomeDaPasta(pref.destino))} <span class="pasta">${esc(paiDaPasta(pref.destino))}</span></span><span class="seta">trocar</span>`;
    b.title = pref.destino;
  } else {
    b.className = "caixa vazia";
    b.innerHTML = "+ ESCOLHER PASTA DE DESTINO";
    b.title = "";
  }
  const t = $("#temas");
  t.classList.toggle("off", !pref.destino);
  t.innerHTML = temas.map((n) => `<button class="tema ${n === pref.tema ? "on" : ""}" data-tema="${esc(n)}">${esc(n)}</button>`).join("") + `<button class="tema novo" id="nova">+ NOVA</button>`;
  $("#rot-temas").classList.toggle("oculto", !pref.destino);
  t.classList.toggle("oculto", !pref.destino);
  renderIr();
}
$("#destino").addEventListener("click", async () => {
  const p = await dialogo.open({ directory: true, title: "Pasta de destino dos sons", defaultPath: pref.destino || undefined }).catch(() => null);
  if (!p) return;
  pref.destino = p; pref.tema = null; salvar();
  carregarTemas();
});
$("#temas").addEventListener("click", (e) => {
  const b = e.target.closest(".tema");
  if (!b) return;
  if (b.id === "nova") {
    b.innerHTML = `<input placeholder="NOME DA PASTA" maxlength="60">`;
    const inp = b.querySelector("input");
    inp.focus();
    const criar = async () => {
      const nome = inp.value.trim();
      if (!nome) { renderDestino(); return; }
      try {
        pref.tema = await invoke("criar_subpasta", { pasta: pref.destino, nome: nome.toUpperCase() });
        salvar();
        await carregarTemas();
      } catch (err) { aviso(`Não deu para criar a pasta: ${err}`); renderDestino(); }
    };
    inp.addEventListener("keydown", (ev) => { if (ev.key === "Enter") criar(); if (ev.key === "Escape") renderDestino(); ev.stopPropagation(); });
    inp.addEventListener("blur", criar, { once: true });
    return;
  }
  if (e.target.closest("input")) return;
  pref.tema = pref.tema === b.dataset.tema ? null : b.dataset.tema;
  salvar(); renderDestino();
});

// ação, formato e parâmetros
const PRINCIPAIS = ["wav", "mxf", "aiff", "mp3"];
const fmt = () => est.formatos.find((f) => f.id === pref.formato) || est.formatos[0];
function renderAcao() {
  document.querySelectorAll("#acao button").forEach((b) => b.classList.toggle("on", b.dataset.acao === pref.acao));
  const conv = pref.acao === "converter";
  $("#conv").classList.toggle("oculto", !conv);
  $("#copia-info").classList.toggle("oculto", conv);
  if (!est.formatos.length) return;
  const outro = !PRINCIPAIS.includes(pref.formato);
  $("#fmts").innerHTML = PRINCIPAIS.map((id) => {
    const f = est.formatos.find((x) => x.id === id);
    return f ? `<button class="f ${pref.formato === id ? "on" : ""}" data-f="${id}">${id === "mxf" ? "MXF AVID" : esc(f.nome)}</button>` : "";
  }).join("") + `<button class="f out ${outro ? "on" : ""}" id="outros">${outro ? esc(fmt().nome) + " ▾" : "+ OUTROS"}</button>`;
  const f = fmt();
  let h = "";
  if (f.taxas && f.taxas.length === 1) h += `<span class="rot">Taxa</span><span class="v" title="${esc(f.nome)} só grava nesta taxa">${f.taxas[0] / 1000} kHz</span>`;
  else h += `<span class="rot">Taxa</span><button class="v" id="p-taxa">${pref.taxa ? virgula(pref.taxa / 1000, pref.taxa % 1000 ? 1 : 0) + " kHz" : "original"}</button>`;
  if (f.comPerda) h += `<span class="rot">Qualidade</span><button class="v" id="p-kbps">${pref.kbps[f.id] || f.kbpsPadrao} kbps</button>`;
  else if (f.bits && f.bits.length > 1) h += `<span class="rot">Bits</span><button class="v" id="p-bits">${f.bits.includes(pref.bits) ? pref.bits : f.bits[f.bits.length - 1]}${pref.bits === 32 ? " float" : ""}</button>`;
  if (f.id === "mxf") h += `<span class="rot">Avid</span><button class="v" id="p-fps">${esc(rotuloFps())} fps</button>`;
  $("#params").innerHTML = h;
  renderIr();
}
function rotuloFps() {
  const v = est.velocidades.find((x) => x.fps.num === pref.fps.num && x.fps.den === pref.fps.den && x.fps.drop === pref.fps.drop);
  return v ? v.rotulo : `${pref.fps.num}/${pref.fps.den}`;
}
$("#acao").addEventListener("click", (e) => {
  const b = e.target.closest("button");
  if (!b || enviando) return;
  pref.acao = b.dataset.acao; salvar(); renderAcao();
});
$("#fmts").addEventListener("click", (e) => {
  const b = e.target.closest("button");
  if (!b || enviando) return;
  if (b.id === "outros") {
    menu(b, est.formatos.filter((f) => !PRINCIPAIS.includes(f.id)).map((f) => [f.nome, f.id === pref.formato, () => { pref.formato = f.id; salvar(); renderAcao(); }]));
    return;
  }
  pref.formato = b.dataset.f; salvar(); renderAcao();
});
$("#params").addEventListener("click", (e) => {
  const b = e.target.closest("button");
  if (!b || enviando) return;
  const f = fmt();
  if (b.id === "p-taxa") {
    const ops = [[null, "original"], ...((f.taxas && f.taxas.length ? f.taxas : [44100, 48000, 96000]).map((t) => [t, `${virgula(t / 1000, t % 1000 ? 1 : 0)} kHz`]))];
    menu(b, ops.map(([v, r]) => [r, pref.taxa === v, () => { pref.taxa = v; salvar(); renderAcao(); }]));
  } else if (b.id === "p-bits") {
    menu(b, f.bits.map((v) => [v === 32 ? "32 float" : `${v} bits`, pref.bits === v, () => { pref.bits = v; salvar(); renderAcao(); }]));
  } else if (b.id === "p-kbps") {
    menu(b, f.kbps.map((v) => [`${v} kbps`, (pref.kbps[f.id] || f.kbpsPadrao) === v, () => { pref.kbps[f.id] = v; salvar(); renderAcao(); }]));
  } else if (b.id === "p-fps") {
    menu(b, est.velocidades.map((v) => [`${v.rotulo} fps`, rotuloFps() === v.rotulo, () => { pref.fps = v.fps; salvar(); renderAcao(); }]));
  }
});
document.querySelectorAll(".chk").forEach((b) => b.addEventListener("click", async () => {
  if (enviando) return;
  const k = b.dataset.op;
  pref[k] = !pref[k]; salvar(); renderOpcoes();
  if (k === "traduzir" || k === "encurtar") { await sugerir(cesta); renderCesta(); }
}));
function renderOpcoes() { document.querySelectorAll(".chk").forEach((b) => b.classList.toggle("on", !!pref[b.dataset.op])); }

function renderIr() {
  const b = $("#ir");
  const n = cesta.filter((c) => c.estado !== "ok").length;
  const conv = pref.acao === "converter";
  const verbo = conv ? "CONVERTER" : "COPIAR";
  const alvo = pastaFinal();
  if (enviando) return;
  b.classList.remove("trabalhando");
  if (!alvo) { b.disabled = true; b.innerHTML = `${verbo} ${n ? plural(n, "SOM", "SONS") : ""}<small>escolha a pasta de destino</small>`; return; }
  if (!n) { b.disabled = true; b.innerHTML = `${verbo}<small>ponha sons na cesta</small>`; return; }
  b.disabled = false;
  const destinoTxt = `${nomeDaPasta(pref.destino)}${pref.tema ? "/" + pref.tema : ""}`;
  const f = fmt();
  b.innerHTML = `${verbo} ${plural(n, "SOM", "SONS")}<small>${conv && f ? `em ${f.id === "mxf" ? "MXF Avid" : esc(f.nome)} ` : ""}→ ${esc(destinoTxt)}</small>`;
}

$("#ir").addEventListener("click", async () => {
  if (enviando) { invoke("cancelar_envio"); return; }
  const alvo = pastaFinal();
  const itens = cesta.filter((c) => c.estado !== "ok");
  if (!alvo || !itens.length) return;
  if (!(await invoke("pasta_existe", { pasta: pref.destino }))) {
    aviso("A pasta de destino não existe mais. Escolha outra.");
    pref.destino = null; pref.tema = null; salvar(); carregarTemas();
    return;
  }
  enviando = true;
  const b = $("#ir");
  b.classList.add("trabalhando");
  itens.forEach((it) => { it.estado = "fila"; it.p = 0; it.msg = null; });
  renderCesta();
  const f = fmt();
  const envio = {
    pasta: alvo, acao: pref.acao, formato: f.id, taxa: f.taxas && f.taxas.length === 1 ? f.taxas[0] : pref.taxa,
    bits: f.bits && f.bits.length ? (f.bits.includes(pref.bits) ? pref.bits : f.bits[f.bits.length - 1]) : 24,
    kbps: f.comPerda ? pref.kbps[f.id] || f.kbpsPadrao : null, fps: pref.fps, origem: !!pref.origem,
  };
  const progresso = new Channel();
  const mostrar = (k) => {
    b.innerHTML = `${pref.acao === "converter" ? "CONVERTENDO" : "COPIANDO"} ${k + 1} DE ${itens.length}<small>clique para parar</small><span class="prog" style="width:${Math.round(((k + (itens[k]?.p || 0)) / itens.length) * 100)}%"></span>`;
  };
  progresso.onmessage = ({ item, p }) => {
    const it = itens[item];
    if (!it) return;
    it.estado = "indo"; it.p = p;
    const el = $(`.item[data-k="${cesta.indexOf(it)}"] .barrinha i`);
    if (el) el.style.width = `${Math.round(p * 100)}%`; else renderCesta();
    mostrar(item);
  };
  mostrar(0);
  let res = [];
  try {
    res = await invoke("enviar", { itens: itens.map((i) => ({ caminho: i.caminho, nome: i.nome, trecho: i.trecho })), envio, progresso });
  } catch (e) { aviso(`Erro: ${e}`); }
  enviando = false;
  let ok = 0;
  const avisos = [];
  for (const r of res) {
    const it = itens.find((i) => i.caminho === r.caminho && i.estado !== "ok" && i.estado !== "erro");
    if (!it) continue;
    if (r.erro) { it.estado = "erro"; it.msg = r.erro; } else { it.estado = "ok"; ok++; avisos.push(...r.avisos); }
  }
  itens.forEach((it) => { if (it.estado === "fila" || it.estado === "indo") it.estado = null; });
  // os que deram certo saem da cesta
  for (let k = cesta.length - 1; k >= 0; k--) if (cesta[k].estado === "ok") cesta.splice(k, 1);
  renderCesta();
  est = { ...est, ...(await invoke("estado").catch(() => ({}))) };
  await buscar();
  if (ok) {
    const extra = avisos.length ? ` · ${[...new Set(avisos)].join(" · ")}` : "";
    aviso(`${plural(ok, "som", "sons")} em ${nomeDaPasta(pref.destino)}${pref.tema ? "/" + pref.tema : ""}${extra}`, "ABRIR PASTA", () => T.opener.revealItemInDir(juntar(alvo, "ORIGEM.txt")).catch(() => T.opener.revealItemInDir(alvo).catch(() => {})));
  }
});

// ---------- avisos ----------
let tAviso = 0;
function aviso(txt, botao, acao) {
  const a = $("#aviso");
  a.innerHTML = `<span>${esc(txt)}</span>${botao ? `<button>${esc(botao)}</button>` : ""}`;
  a.classList.remove("oculto");
  if (botao) a.querySelector("button").onclick = () => { acao(); a.classList.add("oculto"); };
  clearTimeout(tAviso);
  tAviso = setTimeout(() => a.classList.add("oculto"), 7000);
}

// ---------- banco e índice ----------
function quando(seg) {
  if (!seg) return "nunca atualizado";
  const dias = Math.floor((Date.now() / 1000 - seg) / 86400);
  return dias <= 0 ? "índice de hoje" : dias === 1 ? "índice de ontem" : `índice de ${dias} dias atrás`;
}
function renderTopo() {
  if (indexando) $("#sub1").textContent = $("#sub1").textContent || "lendo o banco…";
  else $("#sub1").textContent = est.banco ? `${milhar(est.total)} sons · ${quando(est.atualizadoEm)}` : "nenhum banco escolhido";
  $("#sub2").textContent = est.banco ? `banco: ${nomeDaPasta(est.banco)}` : "banco: —";
  $("#atualizar").textContent = indexando ? "parar" : "atualizar";
  $("#atualizar").disabled = !est.banco && !indexando;
}

async function indexar(pasta) {
  if (indexando) return;
  indexando = true;
  renderTopo();
  const boas = !$("#boas").classList.contains("oculto");
  if (boas) { $("#boas-prog").classList.remove("oculto"); $("#boas-pasta").className = "caixa"; $("#boas-pasta").innerHTML = `<span>${esc(nomeDaPasta(pasta))} <span class="pasta">${esc(paiDaPasta(pasta))}</span></span>`; }
  const canal = new Channel();
  let ultimaBusca = 0;
  canal.onmessage = (p) => {
    let txt, frac = 0;
    if (!p.aLer) txt = `Procurando os sons… ${milhar(p.vistos)} achados`;
    else {
      txt = `Lendo o banco… ${milhar(p.lidos)} de ${milhar(p.aLer)}`;
      frac = p.lidos / p.aLer;
    }
    $("#sub1").textContent = txt.toLowerCase();
    $("#boas-txt").textContent = p.aLer ? "Lendo o banco…" : "Procurando os sons…";
    $("#boas-n").textContent = p.aLer ? `${milhar(p.lidos)} de ${milhar(p.aLer)} sons` : `${milhar(p.vistos)} achados`;
    $("#boas-barra").style.width = `${Math.round(frac * 100)}%`;
    // de tempos em tempos, refaz a busca para mostrar os sons já lidos
    if (p.lidos && Date.now() - ultimaBusca > 4000) { ultimaBusca = Date.now(); recarregarBibs().then(() => { if (consulta().texto) buscar(); else renderBibs(); }); }
  };
  try {
    const r = await invoke("indexar", { pasta, progresso: canal });
    const partes = [`${milhar(r.total)} sons no banco`];
    if (r.novos) partes.push(`${milhar(r.novos)} lidos agora`);
    if (r.removidos) partes.push(`${milhar(r.removidos)} saíram`);
    aviso(`Índice atualizado: ${partes.join(" · ")}.`);
  } catch (e) {
    if (!String(e).includes("cancelado")) aviso(`Não deu para ler o banco: ${e}`);
  }
  indexando = false;
  $("#boas").classList.add("oculto");
  est = { ...est, ...(await invoke("estado").catch(() => ({}))) };
  await recarregarBibs();
  renderTopo();
  buscar();
}
async function recarregarBibs() {
  todasBibs = await invoke("bibliotecas").catch(() => todasBibs);
}
async function escolherBanco() {
  const p = await dialogo.open({ directory: true, title: "Pasta do banco de sons", defaultPath: est.banco || undefined }).catch(() => null);
  if (p) indexar(p);
}
$("#boas-pasta").addEventListener("click", () => { if (!indexando) escolherBanco(); });
$("#boas-fechar").addEventListener("click", () => $("#boas").classList.add("oculto"));
$("#atualizar").addEventListener("click", () => {
  if (indexando) { invoke("cancelar_indice"); return; }
  if (est.banco) indexar(est.banco);
});

// a IA abre em segundo plano junto com o app: quando ficar pronta, refaz a busca já com ela
async function vigiarIa() {
  while (est.ia === "abrindo") {
    await new Promise((ok) => setTimeout(ok, 1500));
    try { est = { ...est, ...(await invoke("estado")) }; } catch (_) { break; }
  }
  if (!$("#ajustes").classList.contains("oculto")) renderAjIa();
  if (usarIa() && consulta().texto) buscar();
}

// ajustes (engrenagem)
function abrirAjustes() {
  const b = $("#aj-banco");
  b.className = est.banco ? "caixa" : "caixa vazia";
  b.innerHTML = est.banco ? `<span>${esc(nomeDaPasta(est.banco))} <span class="pasta">${esc(paiDaPasta(est.banco))}</span></span><span class="seta">trocar</span>` : "+ ESCOLHER A PASTA DO BANCO";
  $("#aj-info").textContent = est.banco ? `${milhar(est.total)} sons · ${quando(est.atualizadoEm)}. Trocar de banco faz o app ler a pasta nova do zero; o seu gosto (👍 👎) continua guardado.` : "";
  $("#aj-atualizar").disabled = !est.banco || indexando;
  renderAjIa();
  $("#versao").textContent = `SFX Finder ${est.versao || ""}`;
  $("#ajustes").classList.remove("oculto");
}
function renderAjIa() {
  const e = est.ia || "";
  const txt = e === "pronta" ? "pronta" : e === "abrindo" ? "abrindo…" : e ? `não abriu (${e})` : "—";
  $("#aj-ia").innerHTML = `<i></i>Usar a IA na busca e nos nomes <small>(Qwen, no computador, sem internet · ${esc(txt)})</small>`;
  $("#aj-ia").classList.toggle("on", !!pref.ia);
}
$("#aj-ia").addEventListener("click", () => { pref.ia = !pref.ia; salvar(); renderAjIa(); buscar(); });
$("#engr").addEventListener("click", abrirAjustes);
$("#aj-fechar").addEventListener("click", () => $("#ajustes").classList.add("oculto"));
$("#aj-banco").addEventListener("click", async () => { $("#ajustes").classList.add("oculto"); if (!indexando) escolherBanco(); });
$("#aj-atualizar").addEventListener("click", () => { $("#ajustes").classList.add("oculto"); if (est.banco) indexar(est.banco); });

// ---------- teclado ----------
function larguraBusca() { const q = $("#q"); q.style.width = q.value ? `${Math.max(8, q.value.length + 2)}ch` : "60%"; }
$("#q").addEventListener("input", () => { larguraBusca(); buscarDepois(); });
$("#q").addEventListener("keydown", (e) => {
  if (e.key === "Enter") { clearTimeout(tBusca); buscar(); }
  if (e.key === "ArrowDown" && resp.sons.length) { e.preventDefault(); $("#q").blur(); if (sel < 0) selecionar(0); }
  if (e.key === "Escape") { $("#q").value = ""; buscar(); }
});
$("#buscar").addEventListener("click", (e) => { e.preventDefault(); clearTimeout(tBusca); buscar(); });
document.querySelectorAll(".chip").forEach((b) => b.addEventListener("click", () => {
  const k = b.dataset.filtro, at = Math.max(0, opAtual(k));
  menu(b, OP_FILTROS[k].ops.map((o, i) => [o[0], i === at, () => aplicarFiltro(k, i)]));
}));
addEventListener("mousedown", (e) => { if (!e.target.closest("#menu") && !e.target.closest(".chip") && !e.target.closest("#outros") && !e.target.closest("#params .v")) fecharMenu(); });
addEventListener("keydown", (e) => {
  if (!$("#menu").classList.contains("oculto") && e.key === "Escape") { fecharMenu(); return; }
  const campo = e.target.closest && e.target.closest("input, textarea");
  if (campo) return;
  if (!$("#boas").classList.contains("oculto") || !$("#ajustes").classList.contains("oculto")) {
    if (e.key === "Escape") { $("#ajustes").classList.add("oculto"); }
    return;
  }
  if (e.key === " ") { e.preventDefault(); tocar(); }
  else if (e.key === "ArrowDown") { e.preventDefault(); selecionar(Math.min(resp.sons.length - 1, sel + 1)); }
  else if (e.key === "ArrowUp") { e.preventDefault(); if (sel <= 0) $("#q").focus(); else selecionar(sel - 1); }
  else if (e.key === "Enter") { e.preventDefault(); const s = somSel(); if (s) alternarCesta(s); }
  else if (e.key === "Escape") parar();
  else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") { e.preventDefault(); $("#q").focus(); $("#q").select(); }
  else if (e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey) { $("#q").focus(); }
});
addEventListener("resize", () => { for (const i of visiveis) desenharMini($(`canvas[data-onda="${i}"]`), i); desenharGrande(); });

// ---------- início ----------
(async () => {
  renderFiltros();
  renderOpcoes();
  renderCab();
  larguraBusca();
  renderCesta();
  try { est = { ...est, ...(await invoke("estado")) }; } catch (e) { aviso(`Erro ao abrir: ${e}`); }
  if (est.erro) dialogo.message(`O SFX Finder não encontrou um programa de que precisa:\n\n${est.erro}`, { title: "SFX Finder", kind: "error" }).catch(() => {});
  if (!est.formatos.find((f) => f.id === pref.formato)) pref.formato = "wav";
  renderAcao();
  // pasta de destino sumiu: volta sem destino (se o disco só demorar a responder, mantém)
  if (pref.destino && (await comTempo(invoke("pasta_existe", { pasta: pref.destino }), 3000, true)) === false) { pref.destino = null; pref.tema = null; salvar(); }
  await carregarTemas();
  renderTopo();
  vigiarIa();
  if (!est.banco || !est.bancoExiste) {
    $("#boas").classList.remove("oculto");
    if (est.banco && !est.bancoExiste) $("#boas p").textContent = `A pasta do banco (${est.banco}) não foi encontrada. O HD está ligado? Escolha a pasta de novo.`;
  }
  await recarregarBibs();
  renderBibs();
  renderLinhas();
  renderPlayerInfo();
  $("#q").focus();
})();
