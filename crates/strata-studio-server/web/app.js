// Strata Studio: the pages served by the server.
// The journey (tablet, phone), the studio screen, the virtual Stream Deck,
// the shoots, the live, the programmes and the administration.
'use strict';
const { h, render } = preact;
const { useState, useEffect, useRef, useMemo, useCallback } = preactHooks;
const html = htm.bind(h);

// ---------- shared ----------

let toastTimer = null;
function toast(text) {
  let el = document.getElementById('toast');
  if (!el) {
    el = document.createElement('div');
    el.id = 'toast';
    el.className = 'toast';
    el.setAttribute('role', 'status');
    document.body.appendChild(el);
  }
  el.textContent = text;
  el.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { el.hidden = true; }, 3200);
}

async function api(path, body, method) {
  const opts = { method: method || (body === undefined ? 'GET' : 'POST'), headers: {} };
  if (body !== undefined && body !== null) {
    opts.headers['content-type'] = 'application/json';
    opts.body = JSON.stringify(body);
  }
  const r = await fetch(path, opts);
  const text = await r.text();
  let data = null;
  try { data = text ? JSON.parse(text) : null; } catch (e) { data = text; }
  if (!r.ok || (data && data.error)) {
    const msg = (data && data.error) || (typeof data === 'string' && data) || r.statusText;
    toast(msg);
    throw new Error(msg);
  }
  return data;
}
const post = (path, body) => api(path, body === undefined ? null : body, 'POST').catch(() => null);

// The snapshot, four times a second (Server-Sent Events), with a fallback.
function useSnapshot() {
  const [snap, setSnap] = useState(null);
  useEffect(() => {
    let es = null, timer = null, dead = false;
    const poll = async () => {
      try { const r = await fetch('/api/snapshot'); setSnap(await r.json()); } catch (e) { /* the server restarts */ }
      if (!dead) timer = setTimeout(poll, 1000);
    };
    try {
      es = new EventSource('/api/live');
      es.onmessage = (e) => { try { setSnap(JSON.parse(e.data)); } catch (x) { /* ignore */ } };
      es.onerror = () => { if (es.readyState === 2 && !timer) poll(); };
    } catch (e) { poll(); }
    return () => { dead = true; if (es) es.close(); clearTimeout(timer); };
  }, []);
  return snap;
}

function useFetch(path, deps) {
  const [data, setData] = useState(null);
  const [n, setN] = useState(0);
  useEffect(() => {
    let live = true;
    if (!path) return undefined;
    fetch(path).then((r) => (r.ok ? r.json() : null)).then((d) => { if (live) setData(d); }).catch(() => {});
    return () => { live = false; };
  }, [path, n, ...(deps || [])]);
  return [data, () => setN((x) => x + 1), setData];
}

function useNow(ms) {
  const [, set] = useState(0);
  useEffect(() => { const t = setInterval(() => set((x) => x + 1), ms || 250); return () => clearInterval(t); }, [ms]);
}

const fmt = (sec) => {
  const t = Math.max(0, Math.floor(sec || 0));
  const hh = Math.floor(t / 3600), m = Math.floor((t % 3600) / 60), s = t % 60;
  return (hh ? hh + ':' + String(m).padStart(2, '0') : String(m).padStart(2, '0')) + ':' + String(s).padStart(2, '0');
};
const hm = (hours) => { if (hours == null) return '—'; const hh = Math.floor(hours); return `${hh} h ${String(Math.floor((hours - hh) * 60)).padStart(2, '0')}`; };
const bytes = (b) => (b == null ? '—' : b >= 1e12 ? (b / 1e12).toFixed(1).replace('.', ',') + ' To' : (b / 1e9).toFixed(0) + ' Go');
const first = (n) => (n || '').trim().split(/\s+/)[0] || n;
const initials = (n, i) => { const p = (n || '').trim().split(/\s+/).filter(Boolean); return p.length ? (p[0][0] + (p[1] ? p[1][0] : '')).toUpperCase() : String(i + 1); };
const personName = (d, i) => ((d.names && d.names[i]) || '').trim() || (i === 0 ? 'Animateur' : 'Invité ' + i);
const dateFr = (ms) => { try { return new Date(ms).toLocaleString('fr-FR', { weekday: 'short', day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' }); } catch (e) { return ''; } };

const I = {
  get arrow() { return html`<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12h14M13 6l6 6-6 6"/></svg>`; },
  get back() { return html`<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M19 12H5M11 6l-6 6 6 6"/></svg>`; },
  get check() { return html`<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg>`; },
  get cam() { return html`<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M15 10l5-3v10l-5-3M3 6h12v12H3z"/></svg>`; },
  get sun() { return html`<svg width="36" height="36" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 8a4 4 0 1 0 0 8a4 4 0 1 0 0-8zM12 2v2M12 20v2M2 12h2M20 12h2M5 5l1.5 1.5M17.5 17.5L19 19M5 19l1.5-1.5M17.5 6.5L19 5"/></svg>`; },
  get stop() { return html`<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" aria-hidden="true"><path d="M6 6h12v12H6z"/></svg>`; },
  ico: (d) => html`<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d=${d}/></svg>`,
};
const P = {
  light: 'M12 8a4 4 0 1 0 0 8a4 4 0 1 0 0-8zM12 2v2M12 20v2M2 12h2M20 12h2',
  cams: 'M15 10l5-3v10l-5-3M3 6h12v12H3z',
  mixer: 'M6 4v16M12 4v16M18 4v16M4 9h4M10 15h4M16 7h4',
  console: 'M4 18h16M7 18V8M12 18V5M17 18v-7',
  mic: 'M12 4a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0V7a3 3 0 0 1 3-3zM6 11a6 6 0 0 0 12 0',
  hub: 'M4 6h16M4 12h16M4 18h16M8 4v16M16 4v16',
  live: 'M5 13a7 7 0 0 1 14 0M8.5 13a3.5 3.5 0 0 1 7 0M12 13v.5',
  disk: 'M4 7h16v10H4zM8 11h.01',
  play: 'M4 6h16v12H4zM10 9l5 3-5 3z',
  frame: 'M4 6h16v12H4z',
};
const CHECK_ICON = { 'Lumière': P.light, 'Caméras': P.cams, 'Mélangeur': P.mixer, 'Console': P.console, 'Micros': P.mic, 'Nodal': P.hub, 'Diffusion': P.live, 'Stockage': P.disk };

// A button acted on by holding it (launch: 1 s, end: 2 s).
function Hold({ ms, onDone, className, children, disabled }) {
  const [p, setP] = useState(0);
  const t0 = useRef(0), raf = useRef(0);
  const stop = () => { cancelAnimationFrame(raf.current); t0.current = 0; setP(0); };
  const tick = () => {
    const v = Math.min(1, (performance.now() - t0.current) / ms);
    setP(v);
    if (v >= 1) { stop(); onDone(); return; }
    raf.current = requestAnimationFrame(tick);
  };
  const down = (e) => { if (disabled) return; e.preventDefault(); t0.current = performance.now(); raf.current = requestAnimationFrame(tick); };
  useEffect(() => () => cancelAnimationFrame(raf.current), []);
  return html`<button type="button" class=${'btn hold ' + (className || '') + (disabled ? ' off' : '')} onPointerDown=${down} onPointerUp=${stop} onPointerLeave=${stop} onPointerCancel=${stop} onKeyDown=${(e) => { if (!disabled && (e.key === 'Enter' || e.key === ' ') && !t0.current) down(e); }} onKeyUp=${stop}>
    <span class="fill" style=${{ width: (p * 100).toFixed(1) + '%' }}></span><span class="in">${children}</span></button>`;
}

// ---------- the journey ----------

const PHASES = [['Démarrer', [1, 2]], ['Programme', [3, 4, 5, 6, 7, 8]], ['Plateau', [9, 10, 11]], ['Tests', [12]], ['Tournage', [13]], ['Fin', [14]]];
const TITLES = { 2: 'Vérification du studio', 3: 'Programme', 4: 'Enregistrement', 5: 'Stockage', 6: 'Résolution', 7: 'Format', 8: 'Diffusion', 9: 'Personnes', 10: 'Flux extérieurs', 11: 'Placement', 12: 'Tests', 13: 'Récapitulatif', 14: 'Fin de tournage' };

function recList(d) { return [d.pgm && 'PGM', d.clean && 'CLEAN', d.iso && '4 caméras', d.mics && 'micros'].filter(Boolean); }
function diffList(s) { const d = s.draft; return [d.go && (s.gos[d.go - 1] || {}).label, d.youtube && 'YouTube', d.tiktok && 'TikTok'].filter(Boolean); }
function extList(d) { return [d.calls && d.calls + ' call' + (d.calls > 1 ? 's' : ''), d.agency && 'agence', d.duplex && 'duplex'].filter(Boolean); }
const resLabel = (d) => (d.resolution === '2160p50' ? '4K' : 'Full HD');

function answers(s, phase) {
  const d = s.draft, st = s.step, A = (k, v) => ({ k, v });
  if (phase === 1 && st > 3) return [A('Nom', d.name), st > 4 && A('Rec', recList(d).join(', ')), st > 5 && A('Vers', (s.storages[d.storage] || {}).label), st > 6 && A('Qualité', resLabel(d)), st > 7 && A('Format', d.vertical ? '16:9 + 9:16' : '16:9'), st > 8 && A('Direct', diffList(s).join(', ') || 'aucun')].filter(Boolean);
  if (phase === 2 && st > 9) return [A('Plateau', d.people + ' pers.'), st > 10 && A('Ext.', extList(d).join(', ') || 'aucun'), st > 11 && A('Places', 'faites')].filter(Boolean);
  if (phase === 3 && st > 12) return [A('', s.tests.length + ' tests OK')];
  if (phase === 4 && s.shooting) return [A('', fmt(s.shooting.seconds))];
  if (phase === 5 && s.ended) return [A('', fmt(s.ended.seconds))];
  return [];
}

function Journey() {
  const s = useSnapshot();
  useNow(250);
  if (!s) return html`<div class="home"><p class="sub">Connexion au studio…</p></div>`;
  if (s.step === 1) return html`<${Home} s=${s} />`;
  const phase = PHASES.findIndex(([, st]) => st.includes(s.step));
  const live = !!s.shooting;
  const eyebrow = `Étape ${s.step} sur 14 · ${live ? 'En tournage' : TITLES[s.step] || ''}`;
  const chips = answers(s, phase).map((a) => (a.k ? a.k + ' · ' : '') + a.v);
  return html`<div class="j">
    <aside class="j-rail">
      <span class="j-brand">Strata <span>Studio</span></span>
      <nav aria-label="Étapes">${PHASES.map(([label], i) => html`<div class=${'j-phase' + (i < phase ? ' done' : i === phase ? ' now' : '') + (i === phase && live ? ' live' : '')}>
        <div class="col"><span class="dot">${i < phase ? I.check : null}</span>${i < PHASES.length - 1 ? html`<span class="line"></span>` : null}</div>
        <div class="txt"><span class="name">${label}</span>${answers(s, i).map((a) => html`<span class="ans"><i>${a.k}</i><b>${a.v}</b></span>`)}</div></div>`)}</nav>
      <span style="flex:1"></span>
      <span style="font-size:13px;color:#8c97b8">${s.studio} · aussi au Stream Deck</span>
    </aside>
    <main class="j-main">
      <header class="j-head">
        ${live ? html`<span class="pulse" style="width:10px;height:10px;border-radius:50%;background:var(--live)"></span>` : null}
        <span class="j-eyebrow">${eyebrow}</span>
        <span class="j-phasepill">${(PHASES[phase] || [''])[0]}</span>
        <div class="j-dots" aria-hidden="true">${Array.from({ length: 14 }, (_, i) => html`<span class=${i + 1 === s.step ? 'cur' : i + 1 < s.step ? 'past' : null}></span>`)}</div>
        <span class="j-progress" aria-hidden="true"><i style=${{ width: (s.step / 14 * 100) + '%' }}></i></span>
        ${chips.length ? html`<div class="j-chips">${chips.map((c) => html`<span>${c}</span>`)}</div>` : null}
      </header>
      ${s.alerts.length ? html`<div class="alert" role="alert"><span>${s.alerts[s.alerts.length - 1]}</span><button type="button" onClick=${() => post('/api/session/alerts/clear')}>OK</button></div>` : null}
      <div class="j-body"><${Step} s=${s} /></div>
      <${Foot} s=${s} />
    </main>
  </div>`;
}

function Home({ s }) {
  const be = s.bootMs != null ? s.bootMs / 1000 : null;
  return html`<div class="home">
    ${be == null ? html`
      <div class="enter" style="display:flex;flex-direction:column;align-items:center;gap:16px;text-align:center">
        <svg width="90" height="90" viewBox="0 0 96 96" fill="none" aria-hidden="true"><rect x="8" y="8" width="80" height="80" rx="24" fill="#b9c6ff"/><path d="M28 36h40M28 48h28M28 60h34" stroke="#0d1424" stroke-width="7" stroke-linecap="round"/></svg>
        <h1>Strata <span>Studio</span></h1>
        <span style="font-size:20px;color:var(--mist)">${s.studio}${s.simulated ? ' · studio simulé' : null}</span>
      </div>
      <button type="button" class="go-btn enter" onClick=${() => post('/api/session/start')}>Commencer un tournage ${I.arrow}</button>` : html`
      <h1 class="enter" style="font-size:44px">On allume le studio…</h1>
      <div class="boot-row">
        <div class="it"><span class=${'lamp' + (be > 0.3 ? ' on' : '')}>${I.sun}</span>Lumière</div>
        ${[1, 2, 3, 4].map((n, i) => html`<div class="it"><span class=${'cm' + (be > 0.7 + i * 0.35 ? ' on' : '')}>${I.cam}</span>Cam ${n}</div>`)}
      </div>`}
    <nav class="links" aria-label="Autres écrans"><a href="/ecran">Écran du studio</a><a href="/deck">Stream Deck</a><a href="/tournages">Tournages</a><a href="/admin/systeme">Administration</a></nav>
  </div>`;
}

function Foot({ s }) {
  const st = s.step;
  const inQ = st >= 2 && st <= 12;
  const showNext = inQ && !(st === 3 && s.fromProgramme && s.fromProgramme === s.draft.name);
  const nextLabel = st === 12 ? 'Récapitulatif' : st === 11 ? 'Tester image et son' : 'Continuer';
  const recapReady = st === 13 && s.recapMs != null && s.recapMs > 500 * (s.recapChecks.length + 1) && !s.recapChecks.some((c) => c.state === 'fail');
  const note = s.shooting ? 'Ni pause ni toux : seul « Terminer » arrête le tournage.' : s.blocked && inQ && st !== 2 ? s.blocked : '';
  return html`<footer class="j-foot">
    ${st >= 3 && st <= 13 && !s.shooting ? html`<button key="back" type="button" class="btn back" onClick=${() => post('/api/session/prev')}>${I.back} Retour</button>` : null}
    <span key="note" class="j-note">${note}</span>
    ${showNext ? html`<button key="next" type="button" class=${'btn' + (s.blocked ? ' off' : '')} onClick=${() => post('/api/session/next')}>${nextLabel} ${I.arrow}</button>` : null}
    ${st === 13 && !s.shooting ? html`<${Hold} key="launch" ms=${1000} className="red" disabled=${!recapReady} onDone=${() => post('/api/session/launch')}>
      <span style="width:12px;height:12px;border-radius:50%;background:currentColor"></span>${recapReady ? 'Maintenir pour lancer' : 'Dernières vérifications…'}<//>` : null}
    ${s.shooting ? html`<${Hold} key="end" ms=${2000} className="redsoft" onDone=${() => post('/api/session/end')}>${I.stop} Terminer le tournage<//>` : null}
    ${st === 14 ? html`<button key="restart" type="button" class="btn" onClick=${() => post('/api/session/reset')}>Nouveau tournage</button>` : null}
  </footer>`;
}

function Step({ s }) {
  switch (s.step) {
    case 2: return html`<${Verif} s=${s} />`;
    case 3: return html`<${Programme} s=${s} />`;
    case 4: return html`<${Rec} s=${s} />`;
    case 5: return html`<${Store} s=${s} />`;
    case 6: return html`<${Res} s=${s} />`;
    case 7: return html`<${Format} s=${s} />`;
    case 8: return html`<${Diff} s=${s} />`;
    case 9: return html`<${People} s=${s} />`;
    case 10: return html`<${Remote} s=${s} />`;
    case 11: return html`<${Place} s=${s} />`;
    case 12: return html`<${Tests} s=${s} />`;
    case 13: return s.shooting ? html`<${Shooting} s=${s} />` : html`<${Recap} s=${s} />`;
    default: return html`<${Fin} s=${s} />`;
  }
}

const draft = (patch) => post('/api/session/draft', patch);

function Verif({ s }) {
  const age = s.checksMs == null ? 0 : s.checksMs;
  const items = s.checks.length ? s.checks : Object.keys(CHECK_ICON).map((label) => ({ label, detail: '', state: 'wait' }));
  const shown = (i) => s.checks.length && age >= 450 * (i + 1);
  const ready = s.checks.length && age >= 450 * items.length;
  const busy = s.gos.filter((g) => g.state === 'live').map((g) => g.label);
  return html`<div class="stack enter">
    <div><h1 class="h1">${ready ? (s.blocked ? 'Un appareil ne répond pas.' : 'Le studio est prêt.') : 'On vérifie le studio.'}</h1>
    <p class="lead">${ready ? (busy.length ? `Tout répond. ${busy.join(', ')} ${busy.length > 1 ? 'sont déjà utilisées' : 'est déjà utilisée'} par un autre studio : grisée.` : 'Tout répond.') : 'Lumière, caméras, mélangeur, son, routes, diffusion et disques.'}</p></div>
    <div class="grid g4">${items.map((c, i) => {
      const st = shown(i) ? c.state : s.checks.length && age >= 450 * i ? 'run' : 'wait';
      return html`<div class=${'check ' + (st === 'run' ? '' : st)}>
        <span class="ic">${st === 'run' ? html`<span class="ring spin"></span>` : null}${I.ico(CHECK_ICON[c.label] || P.frame)}</span>
        <span><b>${c.label}</b><small>${shown(i) ? c.detail : st === 'run' ? 'vérification…' : null}</small></span></div>`;
    })}</div>
  </div>`;
}

function useDebounced(value, send, ms) {
  const [v, setV] = useState(value);
  const t = useRef(null), focused = useRef(false);
  useEffect(() => { if (!focused.current) setV(value); }, [value]);
  const change = (x) => { setV(x); clearTimeout(t.current); t.current = setTimeout(() => send(x), ms || 350); };
  return [v, change, focused];
}

function Programme({ s }) {
  const d = s.draft;
  const [name, setName, focused] = useDebounced(d.name, (x) => draft({ name: x }));
  const fromProg = s.fromProgramme && s.fromProgramme === d.name;
  return html`<div class="stack enter" style="max-width:860px">
    <h1 class="h1">Quel programme tournez-vous ?</h1>
    <input class="input-big" type="text" value=${name} placeholder="Nom du programme" aria-label="Nom du programme"
      onFocus=${() => { focused.current = true; }} onBlur=${() => { focused.current = false; }} onInput=${(e) => setName(e.target.value)} />
    <div class="stack" style="gap:10px"><span class="label">Programmes du studio · leurs réglages sont gardés</span>
      <div class="row">${s.programmes.map((p, i) => html`<button type="button" class=${'pill' + (s.fromProgramme === p.name ? ' on' : '')} onClick=${() => post('/api/session/programme/' + i)}>${p.name}</button>`)}</div></div>
    <div class="stack" style="gap:10px"><span class="label">Style de réalisation</span>
      <div class="row">${s.profiles.map((p, i) => html`<button type="button" class=${'pill' + (d.profile === i ? ' on' : '')} onClick=${() => draft({ profile: i })}>${p}</button>`)}</div></div>
    ${fromProg ? html`<div class="panel enter">
      <b>Réglages de la dernière fois repris</b>
      <div class="row">${[recList(d).join(' + '), (s.storages[d.storage] || {}).label, resLabel(d), d.vertical ? '16:9 + 9:16' : '16:9', diffList(s).join(' + ') || 'pas de direct', d.people + ' personnes', extList(d).join(', ')].filter(Boolean).map((c) => html`<span class="chip">${c}</span>`)}</div>
      <div class="row"><button type="button" class="btn small" onClick=${() => post('/api/session/goto/12')}>Comme la dernière fois : aller aux tests</button>
      <button type="button" class="btn small ghost" onClick=${() => post('/api/session/next')}>Relire les réglages</button></div></div>` : null}
  </div>`;
}

function Rec({ s }) {
  const d = s.draft;
  const OPTS = [['pgm', 'Programme', 'l’émission réalisée, avec habillage', P.play], ['clean', 'Clean', 'la même, sans habillage', P.frame], ['iso', 'Les 4 caméras', 'chaque caméra dans son fichier', P.cams], ['mics', 'Les micros', 'un fichier audio par micro', P.mic]];
  const channels = (d.pgm ? 1 : 0) + (d.clean ? 1 : 0) + (d.iso ? 4 : 0);
  const pct = Math.min(100, ((s.hoursLeft || 0) / 20) * 100);
  return html`<div class="stack enter">
    <h1 class="h1">Qu’enregistre-t-on ?</h1>
    <div class="grid g4">${OPTS.map(([id, t, sub, path]) => html`<button type="button" class=${'card' + (d[id] ? ' on' : '')} aria-pressed=${d[id]} onClick=${() => draft({ [id]: !d[id] })}>
      <span class="ico">${I.ico(path)}</span><span class="t">${t}</span><span class="s">${sub}</span>${d[id] ? html`<span class="tick pop">${I.check}</span>` : null}</button>`)}</div>
    <div class="row"><button type="button" class="pill" onClick=${() => draft({ pgm: true, clean: true, iso: true, mics: true })}>Tout enregistrer</button></div>
    <div class="panel" style="max-width:640px">
      <div class="row" style="align-items:baseline"><span class="big-num">${channels || d.mics ? hm(s.hoursLeft) : '—'}</span><span>${channels || d.mics ? 'd’enregistrement possible' : 'choisissez au moins une image'}</span></div>
      <div class="bar"><i style=${{ width: pct + '%', background: (s.hoursLeft || 0) < 2 ? 'var(--warn)' : 'var(--ok)' }}></i></div>
      <span class="s" style="font-size:14px;color:var(--ink-3)">${channels} image${channels > 1 ? 's' : null} en ${resLabel(d)}${d.mics ? ' + les micros' : null} · ${bytes(s.recFreeBytes)} libres sur le serveur</span>
    </div>
  </div>`;
}

function Store({ s }) {
  const d = s.draft;
  return html`<div class="stack enter" style="max-width:820px">
    <div><h1 class="h1">Où ranger les fichiers après le tournage ?</h1><p class="lead">Studio enregistre sur son serveur, puis copie tout ici, rangé par programme et par date.</p></div>
    ${s.storages.map((st, i) => html`<button type="button" class=${'card' + (d.storage === i ? ' on' : '')} style="flex-direction:row;align-items:center;gap:16px" onClick=${() => draft({ storage: i })}>
      <span class=${'radio' + (d.storage === i ? ' on' : '')}></span>
      <span style="flex:1;display:flex;flex-direction:column;gap:2px;min-width:0"><span class="t" style="font-size:19px">${st.label}</span><span class="s mono" style="overflow:hidden;text-overflow:ellipsis">${st.path}</span></span>
      <span class="s" style="font-weight:700">${st.freeBytes == null ? 'non monté' : bytes(st.freeBytes) + ' libres'}</span></button>`)}
  </div>`;
}

function Res({ s }) {
  const d = s.draft;
  const opts = [['1080p50', 'Full HD', '1080p50 · plus léger', 4], ['2160p50', '4K', '2160p50 · le plus net, idéal pour le 9:16', 8]];
  return html`<div class="stack enter">
    <h1 class="h1">En quelle qualité ?</h1>
    <div class="grid g2" style="max-width:820px">${opts.map(([id, t, sub, cols]) => html`<button type="button" class=${'card' + (d.resolution === id ? ' on' : '')} onClick=${() => draft({ resolution: id })}>
      <span style=${{ display: 'grid', gridTemplateColumns: `repeat(${cols}, 1fr)`, gap: '3px', width: '180px', height: '101px' }}>${Array.from({ length: cols * Math.round(cols * 9 / 16) }, () => html`<span style=${{ borderRadius: '2px', background: d.resolution === id ? 'var(--night)' : 'var(--rule-2)' }}></span>`)}</span>
      <span class="t" style="font-size:32px">${t}</span><span class="s">${sub}</span>${d.resolution === id ? html`<span class="tick pop">${I.check}</span>` : null}</button>`)}</div>
    <p class="lead">Avec ce choix : ${hm(s.hoursLeft)} d’enregistrement possible.</p>
  </div>`;
}

function Format({ s }) {
  const d = s.draft;
  return html`<div class="stack enter">
    <h1 class="h1">Pour quels écrans ?</h1>
    <div class="row split" style="align-items:stretch;gap:22px">
      <div class="stack" style="gap:12px;flex:0 1 330px">
        <div class="card on static" style="flex-direction:row;align-items:center;gap:14px"><span style="width:64px;height:36px;border-radius:6px;border:3px solid var(--night);flex:none"></span><span><span class="t" style="font-size:20px;display:block">16:9</span><span class="s">toujours enregistré</span></span></div>
        <button type="button" class=${'card' + (d.vertical ? ' on' : ' dashed')} style="flex-direction:row;align-items:center;gap:14px" onClick=${() => draft({ vertical: !d.vertical })}>
          <span style="width:22px;height:38px;border-radius:5px;border:3px solid currentColor;flex:none;margin:0 21px"></span>
          <span><span class="t" style="font-size:20px;display:block">9:16 en plus</span><span class="s">${d.vertical ? 'réalisé pour, fabriqué après' : 'touchez pour l’ajouter'}</span></span></button>
      </div>
      <div class="stack" style="gap:12px;flex:1;min-width:0">
        <div class="screen16" style="max-width:480px">
          <div style="position:absolute;left:46%;top:22%;width:21%;aspect-ratio:1;border-radius:50%;background:#3a4058"></div>
          <div style="position:absolute;left:35%;top:62%;width:42%;height:52%;border-radius:999px 999px 0 0;background:#3a4058"></div>
          ${d.vertical ? html`<span class="side" style="left:0"></span><span class="side" style="right:0"></span><span class="zone pop"></span>` : null}
          <span class="src" style="background:rgba(0,0,0,.6)">GROS PLAN</span>
        </div>
        <p class="lead" style="max-width:480px;font-size:16px">${d.vertical ? 'Les caméras cadrent un peu plus large et la personne au centre : elle tient dans la zone 9:16. Tout reste enregistré en 16:9 ; le serveur fabrique les versions 9:16 après le tournage.' : 'Les gros plans sont cadrés pour le 16:9 : la personne peut être décalée dans l’image.'}</p>
        ${d.vertical && d.resolution !== '2160p50' ? html`<div class="alert" style="margin:0;max-width:480px"><span>En Full HD, la version 9:16 sera moins nette. En 4K, elle reste très nette.</span><button type="button" onClick=${() => draft({ resolution: '2160p50' })}>Passer en 4K</button></div>` : null}
      </div>
    </div>
  </div>`;
}

function Diff({ s }) {
  const d = s.draft;
  const socials = [['youtube', 'YouTube', 'en 16:9'], ['tiktok', 'TikTok', d.vertical ? 'en 9:16' : 'en 9:16 · active aussi le 9:16']];
  const list = diffList(s);
  return html`<div class="stack enter">
    <h1 class="h1">On diffuse en direct ?</h1>
    <div class="stack" style="gap:10px"><span class="label">Plateforme GO · une seule</span>
      <div class="grid g4 gos" style="max-width:820px">${s.gos.map((g, i) => {
        const busy = g.state !== 'idle', on = d.go === i + 1;
        return html`<button type="button" class=${'go' + (busy ? ' busy' : on ? ' on' : '')} disabled=${busy && !on} aria-pressed=${on} onClick=${() => draft({ go: on ? 0 : i + 1 })}>
          <b>${g.label}</b><small>${g.state === 'offline' ? 'ne répond pas' : busy ? 'déjà en service' : on ? 'choisie' : 'libre'}</small></button>`;
      })}</div></div>
    <div class="stack" style="gap:10px"><span class="label">Réseaux sociaux</span>
      <div class="grid g2" style="max-width:820px">${socials.map(([id, t, sub]) => html`<button type="button" class=${'card' + (d[id] ? ' on' : '')} style="flex-direction:row;align-items:center;justify-content:space-between" aria-pressed=${d[id]} onClick=${() => draft({ [id]: !d[id] })}>
        <span><span class="t" style="display:block">${t}</span><span class="s">${sub}</span></span><span class=${'switch' + (d[id] ? ' on' : '')}><i></i></span></button>`)}</div></div>
    <p class="lead">${list.length ? `En direct sur ${list.join(', ')}.` : 'Pas de direct : l’émission est seulement enregistrée.'}</p>
  </div>`;
}

// The names are edited here and sent together, a moment after the last key.
function People({ s }) {
  const d = s.draft;
  const [names, setNames] = useState(d.names.slice());
  const editing = useRef(false), t = useRef(null);
  useEffect(() => { if (!editing.current) setNames(d.names.slice()); }, [d.names.join('\u0001')]);
  const edit = (i, v) => {
    const next = names.slice(); next[i] = v; setNames(next);
    clearTimeout(t.current);
    t.current = setTimeout(() => { draft({ names: next }); }, 400);
  };
  return html`<div class="row split enter" style="gap:40px;align-items:flex-start">
    <div class="stack" style="flex:0 1 320px">
      <h1 class="h1" style="font-size:46px">Combien êtes-vous en plateau ?</h1>
      <div class="stepper"><button type="button" aria-label="Une personne de moins" onClick=${() => draft({ people: Math.max(1, d.people - 1) })}>−</button><span class="n">${d.people}</span><button type="button" class="plus" aria-label="Une personne de plus" onClick=${() => draft({ people: Math.min(5, d.people + 1) })}>+</button></div>
      <p class="lead" style="font-size:16px">Animateur compris, jusqu’à 5. Un micro par personne, les autres restent coupés.</p>
    </div>
    <div class="stack" style="flex:1;gap:10px;min-width:0">${Array.from({ length: d.people }, (_, i) => html`<label key=${i} class="person enter" style="cursor:text">
      <span class="badge">${initials(names[i], i)}</span>
      <span style="flex:1;display:flex;flex-direction:column;min-width:0"><span style="font-size:12px;font-weight:700;color:var(--ink-3)">${i === 0 ? 'Animateur' : 'Invité ' + i}</span>
      <input type="text" value=${names[i] || ''} placeholder="Prénom Nom" onFocus=${() => { editing.current = true; }} onBlur=${() => { editing.current = false; }} onInput=${(e) => edit(i, e.target.value)} /></span></label>`)}</div>
  </div>`;
}

function Remote({ s }) {
  const d = s.draft;
  const copy = async (link) => { try { await navigator.clipboard.writeText(link); toast('Lien copié'); } catch (e) { toast(link); } };
  const STATE = { waiting: ['en attente', 'var(--ink-4)'], connected: ['connecté', 'var(--ok-ink)'], unstable: ['connexion instable', 'var(--warn-ink)'], unknown: ['inconnu', 'var(--live)'], 'création': ['création…', 'var(--ink-4)'] };
  return html`<div class="stack enter">
    <h1 class="h1">Des invités à distance ?</h1>
    <div class="grid g3">
      <div class=${'card static wide' + (d.calls ? ' on' : '')}>
        <span class="t" style="font-size:26px">Call</span><span class="s">Studiocall, Teams ou Zoom</span>
        <div class="stepper" style="gap:12px"><button type="button" style="width:52px;height:52px;font-size:26px" aria-label="Un appel de moins" onClick=${() => draft({ calls: Math.max(0, d.calls - 1) })}>−</button><span class="n" style="font-size:56px;width:50px">${d.calls}</span><button type="button" class="plus" style="width:52px;height:52px;font-size:26px" aria-label="Un appel de plus" onClick=${() => draft({ calls: Math.min(4, d.calls + 1) })}>+</button></div>
        <span class="s">${d.calls ? `${d.calls} appel${d.calls > 1 ? 's' : null} en même temps, par StudioEdge` : 'aucun appel'}</span>
      </div>
      ${[['agency', 'Agence de presse', 'une image d’agence en direct'], ['duplex', 'Duplex', 'un reporter en Aviwest']].map(([id, t, sub]) => html`<button type="button" class=${'card' + (d[id] ? ' on' : '')} aria-pressed=${d[id]} onClick=${() => draft({ [id]: !d[id] })}>
        <span class="t" style="font-size:26px">${t}</span><span class="s">${sub}</span><span style="flex:1"></span><span class=${'switch' + (d[id] ? ' on' : '')}><i></i></span></button>`)}
    </div>
    ${s.guests.length ? html`<div class="stack" style="gap:6px;max-width:900px"><span class="label">Le lien à envoyer à chaque invité · valable le temps du tournage</span>
      ${s.guests.slice(0, d.calls).map((g) => { const [txt, col] = STATE[g.state] || [g.state, 'var(--ink-4)']; return html`<div class="tline" style="gap:12px">
        <span style="display:flex;flex-direction:column;gap:2px;flex:1;min-width:0"><b>${g.label}</b><span class="mono" style="font-size:12px;color:var(--ink-3);overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${g.link || '…'}</span></span>
        <button type="button" class="btn small ghost" disabled=${!g.link} onClick=${() => copy(g.link)}>Copier</button>
        <small style=${{ color: col, display: 'flex', alignItems: 'center', gap: '6px' }}><span class="d" style=${{ background: col }}></span>${txt}</small></div>`; })}</div>`
      : html`<p class="lead">Chaque source extérieure arrive sur le mélangeur, et sera testée comme une personne.</p>`}
  </div>`;
}

const SLOTS = [[80, 160], [172, 56], [280, 40], [388, 56], [480, 160]];
const WHERE = ['à gauche', 'au fond, à gauche', 'au fond', 'au fond, à droite', 'à droite'];
const CAMS_XY = { 1: [226, 348], 2: [112, 332], 3: [334, 348], 4: [448, 332] };

function usePlanScale(ref) {
  const [k, setK] = useState(1);
  useEffect(() => {
    const el = ref.current; if (!el) return undefined;
    const ro = new ResizeObserver(() => setK(Math.min(1, el.clientWidth / 560)));
    ro.observe(el); return () => ro.disconnect();
  }, []);
  return k;
}

function Place({ s }) {
  const d = s.draft;
  const [pick, setPick] = useState(-1);
  const wrap = useRef(null);
  const k = usePlanScale(wrap);
  const seats = d.seats.length === 5 ? d.seats : [null, null, null, null, null];
  const slotOf = (p) => seats.indexOf(p);
  const tapSeat = (slot) => {
    const who = seats[slot];
    if (pick >= 0) {
      const cur = seats.slice(); const from = cur.indexOf(pick); const other = cur[slot];
      cur[slot] = pick; if (from >= 0) cur[from] = other;
      draft({ seats: cur }); setPick(-1);
    } else if (who != null && who < d.people) setPick(who);
  };
  const camFor = (slot) => s.cameraForSeat[slot] || 1;
  return html`<div class="stack enter" style="gap:14px">
    <h1 class="h1" style="font-size:46px">Qui s’assoit où ?</h1>
    <div class="row split" style="gap:22px;align-items:flex-start">
      <div ref=${wrap} class="plan-wrap" style="flex:0 1 560px">
        <div class="plan" style=${{ transform: `scale(${k})` }}>
          <div style="position:absolute;left:120px;top:100px;width:320px;height:120px;border-radius:999px;background:#fff;border:1px solid var(--rule);box-shadow:0 10px 30px rgba(20,20,30,.06)"></div>
          <svg width="560" height="380" viewBox="0 0 560 380" style="position:absolute;left:0;top:0;pointer-events:none" aria-hidden="true">${seats.map((p, slot) => {
            if (p == null || p >= d.people) return null;
            const [cx, cy] = CAMS_XY[camFor(slot)] || CAMS_XY[1];
            return html`<line x1=${cx} y1=${cy - 16} x2=${SLOTS[slot][0]} y2=${SLOTS[slot][1] + 30} stroke="#9aa7e6" stroke-width="2" stroke-dasharray="6 5" stroke-linecap="round" />`;
          })}</svg>
          ${Object.entries(CAMS_XY).map(([id, [x, y]]) => html`<div class="cam" style=${{ left: x - 22 + 'px', top: y - 16 + 'px' }}><span>${I.cam}</span>${id === '1' ? 'Large' : 'Cam ' + id}</div>`)}
          ${SLOTS.map(([x, y], slot) => {
            const p = seats[slot]; const has = p != null && p < d.people;
            return html`<button type="button" class=${'seat' + (has ? ' has' : '') + (has && pick === p ? ' picked' : '') + (!has && pick >= 0 ? ' target' : '')} style=${{ left: x - 33 + 'px', top: y - 33 + 'px' }}
              aria-label=${'Place ' + WHERE[slot] + (has ? ' : ' + personName(d, p) : ' : libre')} onClick=${() => tapSeat(slot)}>
              <span class="c">${has ? initials(d.names[p], p) : '+'}</span><span class="l" style=${{ color: has ? 'var(--ink)' : 'var(--ink-4)' }}>${has ? first(personName(d, p)) : 'libre'}</span></button>`;
          })}
        </div>
      </div>
      <div class="stack" style="flex:1;gap:10px;min-width:0">
        <span class="label">1 · Touchez une personne · 2 · touchez sa place</span>
        ${Array.from({ length: d.people }, (_, p) => { const slot = slotOf(p); return html`<button type="button" class=${'person' + (pick === p ? ' on' : '')} onClick=${() => setPick(pick === p ? -1 : p)}>
          <span class="badge">${initials(d.names[p], p)}</span><span style="display:flex;flex-direction:column"><b style="font-size:17px">${personName(d, p)}</b><span style="font-size:13px;color:var(--ink-3)">${slot >= 0 ? WHERE[slot] + ' · Cam ' + camFor(slot) : 'pas encore placé'}</span></span></button>`; })}
        ${pick >= 0 ? html`<p class="lead" style="font-size:15px">Touchez maintenant une place.</p>` : null}
      </div>
    </div>
  </div>`;
}

function Meter({ level, n }) {
  const on = Math.round((level || 0) * n);
  return Array.from({ length: n }, (_, k) => html`<i style=${{ background: k < on ? (k >= n - 2 ? '#ff4d66' : k >= n - 5 ? '#ffc46b' : '#4ade80') : undefined }}></i>`);
}

function Tests({ s }) {
  const d = s.draft;
  const all = s.tests.length && s.testIndex >= s.tests.length;
  const cur = s.tests[Math.min(s.testIndex, s.tests.length - 1)] || {};
  const hearing = s.hearing;
  const remote = cur.seat == null;
  const heard = remote ? (s.testMs || 0) > 1200 : hearing.heard;
  const low = !all && !remote && hearing.low;
  const ask = remote ? (cur.label || '').startsWith('Call') ? 'L’invité est connecté : demandez-lui de dire quelques mots.' : 'Demandez quelques mots, vérifiez l’image.' : `${first(cur.label)}, dites votre nom et votre fonction.`;
  const say = all ? 'Tout est prêt.' : low ? 'Plus près du micro, s’il vous plaît.' : heard ? 'Merci, c’est bien reçu.' : ask;
  const canOk = !all && heard && !low;
  return html`<div class="row split enter" style="gap:24px;align-items:flex-start">
    <div class="stack" style="gap:14px;flex:0 1 560px;min-width:0">
      <div><span style="font-size:14px;font-weight:800;color:var(--blue)">${all ? 'Tous les tests sont faits' : `Test ${s.testIndex + 1} sur ${s.tests.length}`}</span>
      <h1 class="h1" style="font-size:40px;margin-top:6px">${all ? 'Image et son : tout est bon.' : cur.seat ? `${first(cur.label)} face à sa caméra` : cur.label}</h1></div>
      <span class="label" style="font-size:13px;letter-spacing:.12em">CE QUE MONTRE L’ÉCRAN DU STUDIO</span>
      <div class="screen16">
        ${!all && cur.source ? html`<img src=${'/api/preview/' + cur.source} alt=${'Image de ' + cur.source} />` : null}
        ${d.vertical ? html`<span class=${'side' + (s.screen === 'only' ? ' only' : '')} style="left:0"></span><span class=${'side' + (s.screen === 'only' ? ' only' : '')} style="right:0"></span>${s.screen === 'zone' ? html`<span class="zone"></span>` : null}` : null}
        <span class="src">${all ? 'PGM' : cur.source}</span>
        <div class="meter" aria-hidden="true"><${Meter} level=${all ? 0 : hearing.level} n=${16} /></div>
        <div class="say"><span class="pulse" style=${{ width: '10px', height: '10px', borderRadius: '50%', flex: 'none', background: low ? '#ffc46b' : heard ? '#4ade80' : '#b9c6ff' }}></span>${say}</div>
      </div>
      ${d.vertical ? html`<div class="row">${[['zone', 'Zone 9:16'], ['only', '9:16 seul']].map(([id, t]) => html`<button type="button" class=${'pill' + (s.screen === id ? ' on' : '')} onClick=${() => draft({ screen: id })}>${t}</button>`)}</div>` : null}
    </div>
    <div class="stack" style="flex:1;gap:8px;min-width:0">
      ${s.tests.map((t, k) => { const active = !all && k === s.testIndex; const col = t.done ? 'var(--ok-ink)' : active ? (low ? 'var(--warn-ink)' : 'var(--blue)') : 'var(--ink-4)';
        return html`<div class=${'tline' + (active ? ' cur' : '')}><span class="d" style=${{ background: t.done ? 'var(--ok)' : active ? (low ? 'var(--warn)' : 'var(--blue)') : undefined }}></span><b>${t.label}</b><small style=${{ color: col }}>${t.done ? 'OK' : active ? (low ? 'trop faible' : heard ? 'reçu' : 'en test') : 'à faire'}</small></div>`; })}
      ${low ? html`<div class="alert enter" style="margin:0">On entend mal ${first(cur.label)}. Rapprochez son micro (une main entre la bouche et le micro), puis « Refaire ».</div>` : null}
      <div class="row" style="flex-wrap:nowrap">
        <button type="button" class="btn ghost" style="flex:1" onClick=${() => post('/api/session/test/retry')}>Refaire</button>
        <button type="button" class=${'btn' + (canOk ? '' : ' off')} style="flex:1.4" onClick=${() => canOk && post('/api/session/test/ok')}>${all ? 'Fait' : 'Image et son OK'}</button>
      </div>
    </div>
  </div>`;
}

function Recap({ s }) {
  const d = s.draft;
  const cards = [['PROGRAMME', d.name || '—', 3], ['ENREGISTREMENT', recList(d).join(', ') || '—', 4], ['STOCKAGE', (s.storages[d.storage] || {}).label || '—', 5], ['QUALITÉ', resLabel(d) + ' · 50 i/s', 6],
    ['FORMATS', d.vertical ? '16:9 + 9:16' : '16:9', 7], ['DIRECT', diffList(s).join(', ') || 'aucun', 8], ['PLATEAU', d.people + ' personne' + (d.people > 1 ? 's' : ''), 9], ['EXTÉRIEUR', extList(d).join(', ') || 'aucun', 10]];
  const age = s.recapMs || 0;
  return html`<div class="stack enter" style="gap:18px">
    <h1 class="h1" style="font-size:48px">On récapitule.</h1>
    <div class="grid g4">${cards.map(([k, v, n]) => html`<button type="button" class="card" style="gap:4px;padding:14px 16px;border-radius:20px" onClick=${() => post('/api/session/goto/' + n)}>
      <span style="font-size:12px;font-weight:800;letter-spacing:.1em;color:var(--ink-3)">${k}</span><span style="font-size:16px;font-weight:800;line-height:1.3">${v}</span></button>`)}</div>
    <div class="stack" style="gap:8px">${s.recapChecks.map((c, i) => { const ok = age > 500 * (i + 1); const col = c.state === 'fail' ? 'var(--live)' : c.state === 'warn' ? 'var(--warn-ink)' : 'var(--ok-ink)';
      return html`<div class="row" style="gap:12px;font-size:16px;font-weight:600">${ok ? html`<span class="pop" style=${{ width: '26px', height: '26px', borderRadius: '50%', background: c.state === 'ok' ? 'var(--ok-bg)' : c.state === 'warn' ? 'var(--warn-bg)' : 'var(--live-bg)', color: col, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>${c.state === 'ok' ? I.check : '!'}</span>` : html`<span class="spin" style="width:22px;height:22px;margin:2px;border-radius:50%;border:3px solid var(--rule);border-top-color:var(--night)"></span>`}
        <span style=${{ color: ok ? 'var(--ink)' : 'var(--ink-4)' }}>${c.label}${c.detail && ok ? html` <span style="color:var(--ink-3);font-weight:500">· ${c.detail}</span>` : null}</span></div>`; })}</div>
  </div>`;
}

function Shooting({ s }) {
  const manual = s.mode && s.mode.mode === 'manual';
  const shadow = s.mode && s.mode.mode === 'shadow';
  const LIVE_NAMES = { youtube: 'YouTube', tiktok: 'TikTok · 9:16' };
  const streams = [...(s.draft.go ? [[(s.gos[s.draft.go - 1] || {}).label, s.gos[s.draft.go - 1]]] : []), ...Object.entries(s.lives).map(([k, v]) => [LIVE_NAMES[k] || k, v])];
  return html`<div class="row split enter" style="gap:28px;align-items:flex-start">
    <div class="stack" style="gap:14px;flex:0 1 380px">
      <h1 class="h1" style="font-size:46px">${manual ? 'Un humain a la main.' : 'Tout tourne.'}</h1>
      <p class="lead" style="min-height:52px">${manual ? 'Quelqu’un a coupé au pupitre du mélangeur. L’IA attend, elle reprendra seule.' : `L’IA réalise${s.speaker ? ' : ' + s.speaker + ' parle' : null}. Plans larges et plans d’écoute viennent seuls.`}</p>
      <span class="chrono">${fmt(s.shooting.seconds)}</span>
      <div class="tline" style=${{ boxShadow: manual ? '0 0 0 2px var(--warn)' : undefined }}><span style=${{ padding: '6px 12px', borderRadius: '10px', fontSize: '13px', fontWeight: 800, letterSpacing: '.06em', background: manual ? 'var(--warn)' : 'var(--night)', color: manual ? '#1d1200' : '#fff' }}>${manual ? 'MANUEL' : shadow ? 'OMBRE' : 'IA'}</span><b>${manual ? 'Débrayage au pupitre' : s.onAir || 'Réalisation automatique'}</b></div>
    </div>
    <div class="stack" style="flex:1;gap:10px;min-width:0">
      <div class="tline"><span class="d pulse" style="background:var(--live);width:12px;height:12px"></span><b>Enregistrement</b><small style="color:var(--ink-2);font-weight:600">${recList(s.draft).join(', ')} · ${resLabel(s.draft)}</small></div>
      ${streams.map(([name, st]) => html`<div class="tline"><b>${name}</b><small style=${{ color: st && (st.state === 'live') ? 'var(--ok-ink)' : 'var(--live)' }}>${st && st.state === 'live' ? 'en ligne' : st && st.state === 'failed' ? 'décroché' : '…'}</small></div>`)}
      <div class="panel" style="gap:8px;padding:14px 16px"><b>Son</b>${s.levels.map((l) => html`<div class="row" style="gap:10px;flex-wrap:nowrap"><span style="width:90px;font-size:14px;font-weight:700;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${first(l.name)}</span><span class="bar" style="flex:1;height:8px"><i style=${{ width: Math.max(4, Math.min(100, (l.levelDb + 60) / 60 * 100)) + '%', transition: 'width .15s' }}></i></span></div>`)}</div>
    </div>
  </div>`;
}

function Fin({ s }) {
  const e = s.ended || { seconds: 0, files: 0 };
  const a = s.after || {};
  const age = (s.endMs || 0) / 1000;
  const d = s.draft;
  const store = (s.storages[d.storage] || {}).label || 'le stockage';
  const pct = a.total ? Math.round(a.done * 100 / a.total) : 0;
  const finished = a.step === 'terminé';
  const items = [
    ['Enregistrement arrêté', 'ok'],
    ...(d.vertical ? [['Versions 9:16 fabriquées', finished || a.step === 'copie' ? 'ok' : a.step === 'versions 9:16' ? 'run' : 'wait', a.step === 'versions 9:16' ? `${a.done}/${a.total}` : '']] : []),
    ['Copie vers ' + store, finished ? 'ok' : a.step === 'copie' ? 'run' : 'wait', a.step === 'copie' ? pct + ' %' : ''],
    ['Caméras en veille', age > 3 ? 'ok' : 'run'],
    ['Lumière éteinte', age > 3.5 ? 'ok' : 'run'],
  ];
  return html`<div class="row split" style="gap:34px;align-items:flex-start">
    <div class="stack" style="gap:18px;flex:0 1 340px">
      <svg width="110" height="110" viewBox="0 0 120 120" fill="none" aria-hidden="true"><circle cx="60" cy="60" r="52" stroke="#22a35f" stroke-width="6"/><path d="M38 62l15 15 30-32" stroke="#22a35f" stroke-width="7" stroke-linecap="round" stroke-linejoin="round"/></svg>
      <h1 class="h1 enter" style="font-size:56px">C’est dans la boîte.</h1>
      <p class="lead">${d.name} · ${fmt(e.seconds)} · ${e.files} fichiers. Le profil du programme est gardé pour la prochaine fois.</p>
      ${a.error ? html`<div class="alert" style="margin:0">${a.error}</div>` : null}
      ${e.id ? html`<a href=${'/tournages/' + e.id} style="font-weight:700;color:var(--lilac-ink)">Voir le tournage, sa frise et ses exports</a>` : null}
    </div>
    <div class="stack" style="flex:1;gap:10px;min-width:0">${items.map(([t, st, extra]) => html`<div class="panel" style="padding:14px 16px;gap:8px">
      <div class="row" style="gap:12px;flex-wrap:nowrap">${st === 'ok' ? html`<span class="pop" style="width:26px;height:26px;border-radius:50%;background:var(--ok-bg);color:var(--ok-ink);display:flex;align-items:center;justify-content:center">${I.check}</span>` : st === 'run' ? html`<span class="spin" style="width:22px;height:22px;margin:2px;border-radius:50%;border:3px solid var(--rule);border-top-color:var(--night)"></span>` : html`<span style="width:26px;display:flex;justify-content:center"><span style="width:9px;height:9px;border-radius:50%;background:var(--rule-2)"></span></span>`}
      <b style=${{ flex: 1, color: st === 'wait' ? 'var(--ink-4)' : 'var(--ink)' }}>${t}</b><span style="font-size:14px;color:var(--ink-3)">${extra || ''}</span></div>
      ${t.startsWith('Copie') && st === 'run' ? html`<div class="bar" style="height:6px"><i style=${{ width: pct + '%', background: 'var(--night)' }}></i></div>` : null}</div>`)}</div>
  </div>`;
}

// ---------- the studio screen ----------

const TIPS = [
  ['Regardez votre caméra', 'Celle dont le voyant est rouge vous filme. Parlez à la personne, la caméra suit.'],
  ['Le micro à une main', 'Une main entre la bouche et le micro : ni plus loin, ni plus près.'],
  ['Évitez les rayures fines', 'Les petits motifs et les rayures serrées vibrent à l’image.'],
];
const TIPS_916 = [['Restez au centre', 'Le format vertical garde le milieu de l’image : évitez de vous pencher de côté.'], TIPS[1], ['Des gestes près du corps', 'En 9:16, les mains loin du corps sortent du cadre.']];

function Screen() {
  const s = useSnapshot();
  if (!s) return html`<div class="scr"></div>`;
  const tips = s.draft.vertical ? TIPS_916 : TIPS;
  let body;
  if (s.shooting) {
    body = html`<span class="onair pulse">À L’ANTENNE</span><span class="clock">${fmt(s.shooting.seconds)}</span><span class="sub">${s.draft.name}${s.speaker ? ' · ' + s.speaker : null}</span>`;
  } else if (s.step === 12 && s.tests.length && s.testIndex < s.tests.length) {
    const t = s.tests[s.testIndex];
    const remote = t.seat == null;
    const heard = remote ? (s.testMs || 0) > 1200 : s.hearing.heard;
    const say = !remote && s.hearing.low ? 'Plus près du micro, s’il vous plaît.' : heard ? 'Merci, c’est bien reçu.' : remote ? 'Quelques mots, s’il vous plaît.' : `${first(t.label)}, dites votre nom et votre fonction.`;
    body = html`<div class="scam"><img src=${'/api/preview/' + t.source} alt="" />
      ${s.draft.vertical ? html`<span class=${'side' + (s.screen === 'only' ? ' only' : '')} style="position:absolute;left:0;top:0;bottom:0;width:34.2%;background:rgba(0,0,0,.55)"></span><span style="position:absolute;right:0;top:0;bottom:0;width:34.2%;background:rgba(0,0,0,.55)"></span><span style="position:absolute;left:34.2%;top:0;bottom:0;width:31.6%;border:.4vh dashed #ffd27a"></span>` : null}
      <div class="meter"><${Meter} level=${s.hearing.level} n=${20} /></div>
      <div class="say"><span class="pulse" style=${{ width: '2vh', height: '2vh', borderRadius: '50%', background: s.hearing.low ? '#ffc46b' : heard ? '#4ade80' : '#b9c6ff' }}></span>${say}</div></div>`;
  } else if (s.step === 14) {
    body = html`<h1>C’est dans la boîte.</h1><span class="sub">Merci à tous. Le studio range les fichiers et s’éteint.</span>`;
  } else if (s.step === 1) {
    body = html`<h1>${s.studio}</h1><span class="sub">Bienvenue. Le tournage commence depuis la tablette ou le Stream Deck.</span><div class="tips">${tips.map(([t, x]) => html`<div class="tip"><b>${t}</b><span>${x}</span></div>`)}</div>`;
  } else {
    body = html`<span class="sub" style="letter-spacing:.14em;text-transform:uppercase;font-weight:800">Préparation · ${TITLES[s.step] || ''}</span><h1>${s.draft.name || 'On prépare le tournage.'}</h1>
      <span class="sub">${s.step === 13 ? 'Tout est prêt : le tournage va commencer.' : 'Installez-vous : quelques conseils pour l’image.'}</span><div class="tips">${tips.map(([t, x]) => html`<div class="tip"><b>${t}</b><span>${x}</span></div>`)}</div>`;
  }
  return html`<div class="scr"><div class="scr-in">${body}</div></div>`;
}

// ---------- the virtual Stream Deck ----------

function Deck() {
  const s = useSnapshot();
  if (!s) return html`<div class="dark"></div>`;
  const v = s.deck;
  const cls = (k, i) => (k.on === '1' ? ' on' : k.on === 'x' ? ' x' : !k.label ? ' empty' : i === 6 && k.label === 'Retour' ? ' nav' : i === 7 && k.label ? ' ok' : '');
  return html`<div class="dark" style="min-height:100vh;display:flex;flex-direction:column;align-items:center;gap:22px;padding:30px 16px">
    <div style="text-align:center"><div class="w-title">${v.title}</div><div class="w-sub" style="margin-top:6px">${v.hint}</div></div>
    <div class="deck">
      <div class="keys">${v.keys.map((k, i) => html`<button type="button" class=${'key' + cls(k, i)} disabled=${!k.label || k.on === 'x'} onClick=${() => post('/api/deck/key/' + (i + 1))}>${k.label}${k.sub ? html`<small>${k.sub}</small>` : null}</button>`)}</div>
      <div class="strip">${v.dials.map((d) => html`<div><small>${d.label}</small>${d.value}</div>`)}</div>
      <div class="dials">${v.dials.map((d, i) => html`<div><button type="button" aria-label=${d.label + ' moins'} onClick=${() => post(`/api/deck/dial/${i + 1}/-1`)}>‹</button><button type="button" aria-label=${d.label + ' plus'} onClick=${() => post(`/api/deck/dial/${i + 1}/1`)}>›</button></div>`)}</div>
    </div>
    <p class="w-sub" style="max-width:560px;text-align:center">Le même Stream Deck +, en vrai, passe par Bitfocus Buttons : chaque touche affiche la variable <span class="mono">$(custom:studio_k1)</span>… et envoie <span class="mono">POST /api/deck/key/1</span>.</p>
  </div>`;
}

// ---------- the web pages ----------

const NAV = [['/tournages', 'Tournages'], ['/direct', 'Direct'], ['/programmes', 'Programmes'], null, ['/admin/studio', 'Studio'], ['/admin/realisation', 'Réalisation'], ['/admin/systeme', 'Système']];

function Web({ path, children, studio }) {
  return html`<div class="dark">
    <header class="w-head">
      <a href="/" class="w-brand" style="color:inherit;text-decoration:none"><b>Strata</b> <span>Studio</span></a>
      <nav class="w-nav" aria-label="Pages">${NAV.map((n) => (n ? html`<a href=${n[0]} class=${path.startsWith(n[0]) ? 'on' : null}>${n[1]}</a>` : html`<span class="sep"></span>`))}</nav>
      <span style="flex:1"></span><a class="wbtn" href="/">Tablette</a><a class="wbtn" href="/ecran">Écran</a>
      <span class="w-sub">${studio || ''}</span>
    </header>
    <main class="w-main">${children}</main>
  </div>`;
}

const SRC_COLORS = { CAM1: '#5b6178', CAM2: '#8fa3ff', CAM3: '#4ade80', CAM4: '#fb923c', PGM: '#f472b6', CLEAN: '#38bdf8' };
function Strip({ segments, duration }) {
  if (!duration) return html`<div class="strip"></div>`;
  return html`<div class="strip">${segments.map((sg, i) => {
    const end = i + 1 < segments.length ? segments[i + 1].startMs : duration;
    return html`<i title=${sg.label} style=${{ left: (sg.startMs / duration * 100) + '%', width: (Math.max(0, end - sg.startMs) / duration * 100) + '%', background: SRC_COLORS[sg.source] || '#a3a8bf' }}></i>`;
  })}</div>`;
}

function Shoots() {
  const [list, reload] = useFetch('/api/tournages');
  const [q, setQ] = useState('');
  const shown = (list || []).filter((r) => !q || (r.programme + ' ' + r.date).toLowerCase().includes(q.toLowerCase()));
  return html`<${Web} path="/tournages">
    <div class="row" style="justify-content:space-between"><h1 class="w-title">Tournages</h1>
      <div class="row"><input class="winput" style="width:260px" type="search" placeholder="Programme ou date" aria-label="Chercher" value=${q} onInput=${(e) => setQ(e.target.value)} /><button type="button" class="wbtn" onClick=${reload}>Actualiser</button></div></div>
    ${list && !list.length ? html`<div class="box"><b>Aucun tournage pour l’instant.</b><span class="w-sub">Ils apparaissent ici dès qu’un tournage est lancé depuis la tablette, le téléphone ou le Stream Deck.</span></div>` : null}
    <div class="shoots">${shown.map((r) => html`<a class="shoot" href=${'/tournages/' + r.id}>
      <div class="row" style="justify-content:space-between"><b>${r.programme}</b><span class=${'tag' + (r.status === 'en cours' ? ' live' : ' ok')}>${r.status}</span></div>
      <span class="w-sub">${dateFr(r.startedUnixMs)} · ${fmt(r.durationMs / 1000)} · ${r.answers.people} personnes</span>
      <${Strip} segments=${r.segments} duration=${r.durationMs} />
      <span class="w-sub">${r.segments.length} plans · ${r.files.length} fichiers · ${r.storage || 'serveur'}</span></a>`)}</div>
  <//>`;
}

function Shoot({ id }) {
  const [r] = useFetch('/api/tournages/' + id);
  if (!r) return html`<${Web} path="/tournages"><p class="w-sub">Chargement…</p><//>`;
  const dur = r.durationMs || 1;
  const sources = [...new Set(r.segments.map((s) => s.source))];
  const del = async () => { try { await api('/api/tournages/' + id, null, 'DELETE'); location.href = '/tournages'; } catch (e) { /* toast shown */ } };
  return html`<${Web} path="/tournages">
    <div class="row" style="justify-content:space-between"><div><h1 class="w-title">${r.programme}</h1><span class="w-sub">${dateFr(r.startedUnixMs)} · ${fmt(r.durationMs / 1000)} · ${r.profile.name} · ${r.status}</span></div>
      <div class="row">${[['edl', 'EDL'], ['fcpxml', 'Final Cut / Resolve'], ['xml', 'Premiere'], ['otio', 'OTIO']].map(([f, t]) => html`<a class="wbtn" href=${`/api/tournages/${id}/export/${f}`} download>${t}</a>`)}</div></div>
    <div class="box"><h2>La frise</h2>
      <span class="w-sub">À l’image</span><${Strip} segments=${r.segments} duration=${dur} />
      <div class="legend">${sources.map((s) => html`<span><i style=${{ background: SRC_COLORS[s] || '#a3a8bf' }}></i>${s}</span>`)}</div>
      <span class="w-sub">Qui parle</span>
      <div class="strip">${r.speakers.map((sp) => html`<i title=${sp.name} style=${{ left: sp.startMs / dur * 100 + '%', width: (sp.endMs - sp.startMs) / dur * 100 + '%', background: '#4ade80', opacity: 0.8 }}></i>`)}</div>
      <div class="row w-sub" style="justify-content:space-between"><span>0:00</span><span>${fmt(dur / 2000)}</span><span>${fmt(dur / 1000)}</span></div>
    </div>
    <div class="cols">
      <div class="box"><h2>Ce que l’IA a décidé</h2><div class="journal">${r.notes.map((n) => html`<div class=${n.sent ? '' : 'off'}><span class="t">${fmt(n.tMs / 1000)}</span><span>${n.text}</span></div>`)}</div></div>
      <div class="box"><h2>Ce que l’animateur a dit</h2>${r.heard.length ? html`<div class="journal">${r.heard.map((n) => html`<div><span class="t">${fmt(n.tMs / 1000)}</span><span>${n.text}</span></div>`)}</div>` : html`<span class="w-sub">Pas de transcription (l’écoute de l’animateur n’était pas branchée).</span>`}</div>
    </div>
    <div class="cols">
      <div class="box"><h2>Fichiers</h2><table class="wt"><tbody>${r.files.map((f) => html`<tr><td class="mono" style="word-break:break-all">${f}</td></tr>`)}</tbody></table>
        <span class="w-sub">Copiés vers ${r.storage || '—'} ${r.storagePath ? html`<span class="mono">(${r.storagePath})</span>` : null}</span></div>
      <div class="box"><h2>Chiffres</h2><table class="wt"><tbody>
        <tr><td>Plans</td><td class="mono">${r.segments.length}</td></tr>
        <tr><td>Temps à la main d’un humain</td><td class="mono">${fmt(r.manualMs / 1000)}</td></tr>
        <tr><td>Mode ombre : coupes du réalisateur suivies par l’IA</td><td class="mono">${r.agreement.humanCuts ? Math.round(r.agreement.matched * 100 / r.agreement.humanCuts) + ' %' : '—'}</td></tr>
      </tbody></table><button type="button" class="wbtn danger" onClick=${del}>Supprimer de l’historique</button></div>
    </div>
  <//>`;
}

function Live() {
  const s = useSnapshot();
  const [rec] = useFetch('/api/record/status', [s && s.shooting ? 1 : 0]);
  if (!s) return html`<${Web} path="/direct"><p class="w-sub">Connexion…</p><//>`;
  const mode = s.mode.mode === 'auto' ? 'IA' : s.mode.mode === 'shadow' ? 'OMBRE' : 'MANUEL';
  return html`<${Web} path="/direct" studio=${s.studio}>
    <div class="row" style="justify-content:space-between"><div><h1 class="w-title">Direct</h1><span class="w-sub">${s.shooting ? `${s.draft.name} · ${fmt(s.shooting.seconds)}` : 'Pas de tournage en cours · étape ' + s.step}</span></div>
      <div class="row"><span class=${'tag ' + (mode === 'IA' ? 'ok' : 'warn')}>${mode}</span>
        <button type="button" class="wbtn" onClick=${() => post('/api/control/auto')}>Rendre la main à l’IA</button>
        <button type="button" class="wbtn" onClick=${() => post('/api/control/shadow')}>Mode ombre</button>
        <button type="button" class="wbtn" onClick=${() => post('/api/control/take?by=r%C3%A9gie')}>Prendre la main</button></div></div>
    <div class="cols">
      <div class="box" style="flex:2 1 520px"><div class="preview"><img src="/api/preview/PGM" alt="Programme" /><span class=${'tag onair ' + (s.shooting ? 'live' : '')}>${s.shooting ? 'ENREGISTREMENT' : 'PGM'}</span></div>
        <div class="row" style="justify-content:space-between"><b>${s.onAir || '—'}</b><span class="w-sub">${s.speaker ? s.speaker + ' parle' : null}</span></div></div>
      <div class="box"><h2>Son</h2>${s.levels.map((l) => html`<div class="lvl"><b>${l.name}</b><span class="m"><i style=${{ width: Math.max(2, Math.min(100, (l.levelDb + 60) / 60 * 100)) + '%', background: l.voice ? '#22c55e' : '#3a4058' }}></i></span></div>`)}
        <h2>Diffusion</h2>${s.gos.map((g) => html`<div class="row" style="justify-content:space-between"><span>${g.label}</span><span class=${'tag ' + (g.state === 'live' ? 'live' : g.state === 'offline' ? 'warn' : '')}>${g.state === 'live' ? 'en service' : g.state === 'offline' ? 'hors ligne' : 'libre'}</span></div>`)}
        ${Object.entries(s.lives).map(([k, v]) => html`<div class="row" style="justify-content:space-between"><span>${k === 'youtube' ? 'YouTube' : 'TikTok'}</span><span class=${'tag ' + (v.state === 'live' ? 'live' : 'warn')}>${v.state === 'live' ? fmt(v.seconds) : 'décroché'}</span></div>`)}
        <h2>Enregistrement</h2>${rec && rec.channels && rec.channels.length ? rec.channels.map(([n, c]) => html`<div class="row" style="justify-content:space-between"><span>${n}</span><span class="mono">${c.state} ${c.bytes ? bytes(c.bytes) : null}</span></div>`) : html`<span class="w-sub">rien en cours</span>`}</div>
    </div>
    <div class="box"><h2>Journal</h2><div class="journal">${s.journal.map((e) => html`<div class=${e.sent ? '' : 'off'}><span class="t">${fmt(e.tMs / 1000)}</span><span>${e.text}</span></div>`)}</div></div>
  <//>`;
}

function Programmes() {
  const [list, , setList] = useFetch('/api/settings');
  const [snap] = useFetch('/api/snapshot');
  if (!list) return html`<${Web} path="/programmes"><p class="w-sub">Chargement…</p><//>`;
  const progs = list.programmes;
  const set = (i, k, v) => { const p = progs.map((x, j) => (j === i ? { ...x, [k]: v } : x)); setList({ ...list, programmes: p }); };
  const save = async () => { try { await api('/api/programmes', progs, 'PUT'); toast('Programmes enregistrés'); } catch (e) { /* shown */ } };
  const add = () => setList({ ...list, programmes: [...progs, { name: 'Nouveau programme', profile: 0, pgm: true, clean: false, iso: true, mics: true, storage: 0, resolution: '1080p50', vertical: false, go: 0, youtube: false, tiktok: false, people: 3, names: ['', '', '', '', ''], calls: 0, agency: false, duplex: false, seats: [] }] });
  const del = (i) => setList({ ...list, programmes: progs.filter((_, j) => j !== i) });
  const chk = (i, k, t) => html`<label class="row" style="gap:6px;font-size:13px"><input type="checkbox" checked=${progs[i][k]} onChange=${(e) => set(i, k, e.target.checked)} />${t}</label>`;
  return html`<${Web} path="/programmes" studio=${snap && snap.studio}>
    <div class="row" style="justify-content:space-between"><div><h1 class="w-title">Programmes</h1><span class="w-sub">Choisir un programme sur la tablette reprend tous ses réglages.</span></div>
      <div class="row"><button type="button" class="wbtn" onClick=${add}>Ajouter</button><button type="button" class="wbtn pri" onClick=${save}>Enregistrer</button></div></div>
    ${progs.map((p, i) => html`<div class="box">
      <div class="fields">
        <label class="field">Nom<input value=${p.name} onInput=${(e) => set(i, 'name', e.target.value)} /></label>
        <label class="field">Style de réalisation<select value=${p.profile} onChange=${(e) => set(i, 'profile', +e.target.value)}>${list.profiles.map((pr, k) => html`<option value=${k}>${pr.name}</option>`)}</select></label>
        <label class="field">Stockage<select value=${p.storage} onChange=${(e) => set(i, 'storage', +e.target.value)}>${list.storages.map((st, k) => html`<option value=${k}>${st.label}</option>`)}</select></label>
        <label class="field">Qualité<select value=${p.resolution} onChange=${(e) => set(i, 'resolution', e.target.value)}><option value="1080p50">Full HD 1080p50</option><option value="2160p50">4K 2160p50</option></select></label>
        <label class="field">GO<select value=${p.go} onChange=${(e) => set(i, 'go', +e.target.value)}><option value="0">aucune</option>${list.gos.map((g, k) => html`<option value=${k + 1}>${g.label}</option>`)}</select></label>
        <label class="field">Personnes<input type="number" min="1" max="5" value=${p.people} onInput=${(e) => set(i, 'people', Math.max(1, Math.min(5, +e.target.value || 1)))} /></label>
        <label class="field">Calls<input type="number" min="0" max="4" value=${p.calls} onInput=${(e) => set(i, 'calls', Math.max(0, Math.min(4, +e.target.value || 0)))} /></label>
      </div>
      <div class="row" style="gap:16px">${chk(i, 'pgm', 'PGM')}${chk(i, 'clean', 'CLEAN')}${chk(i, 'iso', '4 caméras')}${chk(i, 'mics', 'micros')}${chk(i, 'vertical', '9:16')}${chk(i, 'youtube', 'YouTube')}${chk(i, 'tiktok', 'TikTok')}${chk(i, 'agency', 'agence')}${chk(i, 'duplex', 'duplex')}
        <span style="flex:1"></span><button type="button" class="wbtn danger" onClick=${() => del(i)}>Retirer</button></div>
    </div>`)}
  <//>`;
}

// ---------- the administration: the studio in nodes ----------

const KINDS = {
  trigger: { color: '#8fa3ff', tag: 'Déclencheur' },
  condition: { color: '#f5a524', tag: 'Condition' },
  action: { color: '#22c55e', tag: 'Action' },
};
const EVENTS = { start: 'Commencer un tournage', launch: 'Lancer l’enregistrement', end: 'Terminer', alert: 'Une alerte', manual: 'Reprise au pupitre' };
function kindOf(n) { return n.type === 'trigger' ? 'trigger' : ['ifLive', 'ifGo', 'ifVertical', 'ifSeat'].includes(n.type) ? 'condition' : 'action'; }
function nodeText(n) {
  switch (n.type) {
    case 'trigger': return EVENTS[n.on] || n.on;
    case 'ifLive': return 'Si une diffusion est choisie';
    case 'ifGo': return 'Si un GO est choisi';
    case 'ifVertical': return 'Si le 9:16 est choisi';
    case 'ifSeat': return `Si la place ${n.seat} est occupée`;
    case 'press': return `Buttons · ${n.label || 'bouton'} (${n.page}/${n.row}/${n.column})`;
    case 'variable': return `Variable ${n.name} = « ${n.value} »`;
    case 'route': return `Nodal · ${n.source} → ${n.destination === '{GO}' ? 'GO choisi' : n.destination}`;
    case 'camerasPower': return n.on ? 'Caméras : allumer' : 'Caméras : veille';
    case 'cameraPreset': return `Cam ${n.camera} · preset ${n.preset}`;
    case 'mics': return n.on ? 'Console : ouvrir les micros occupés' : 'Console : couper les micros';
    case 'wait': return `Attendre ${n.ms} ms`;
    default: return n.type;
  }
}
const PALETTE = [
  ['Déclencheurs', [{ type: 'trigger', on: 'start' }, { type: 'trigger', on: 'launch' }, { type: 'trigger', on: 'end' }, { type: 'trigger', on: 'alert' }, { type: 'trigger', on: 'manual' }]],
  ['Conditions', [{ type: 'ifGo' }, { type: 'ifLive' }, { type: 'ifVertical' }, { type: 'ifSeat', seat: 1 }]],
  ['Actions', [{ type: 'press', page: 1, row: 0, column: 0, label: 'Bouton' }, { type: 'variable', name: 'studio_onair', value: 'ON AIR' }, { type: 'route', destination: '{GO}', source: 'PGM' }, { type: 'camerasPower', on: true }, { type: 'cameraPreset', camera: 1, preset: 1 }, { type: 'mics', on: true }, { type: 'wait', ms: 1000 }]],
];

function Nodes() {
  const [settings, , setSettings] = useFetch('/api/settings');
  const [sys] = useFetch('/api/system');
  const [cur, setCur] = useState(0);
  const [sel, setSel] = useState(null);
  const [arm, setArm] = useState(null);
  const [plan, setPlan] = useState(null);
  const drag = useRef(null);
  if (!settings) return html`<${Web} path="/admin/studio"><p class="w-sub">Chargement…</p><//>`;
  const scs = settings.scenarios;
  const sc = scs[cur] || { nodes: [], wires: [] };
  const update = (f) => { const next = scs.map((x, i) => (i === cur ? f({ ...x, nodes: x.nodes.slice(), wires: x.wires.slice() }) : x)); setSettings({ ...settings, scenarios: next }); };
  const node = sc.nodes.find((n) => n.id === sel);
  const save = async () => { try { await api('/api/scenarios', scs, 'PUT'); toast('Automations enregistrées'); } catch (e) { /* shown */ } };
  const add = (tpl) => update((x) => { const id = 'n' + Date.now().toString(36); x.nodes.push({ ...tpl, id, x: 40 + Math.random() * 200, y: 40 + Math.random() * 300 }); setSel(id); return x; });
  const del = () => update((x) => { x.nodes = x.nodes.filter((n) => n.id !== sel); x.wires = x.wires.filter((w) => w.from !== sel && w.to !== sel); setSel(null); return x; });
  const setField = (k, v) => update((x) => { x.nodes = x.nodes.map((n) => (n.id === sel ? { ...n, [k]: v } : n)); return x; });
  const port = (id, out) => {
    if (out) { setArm(id); return; }
    if (arm && arm !== id) update((x) => { if (!x.wires.some((w) => w.from === arm && w.to === id)) x.wires.push({ from: arm, to: id }); return x; });
    setArm(null);
  };
  const down = (e, n) => { if (e.target.classList.contains('port')) return; setSel(n.id); drag.current = { id: n.id, dx: e.clientX - n.x, dy: e.clientY - n.y }; e.currentTarget.setPointerCapture(e.pointerId); };
  const move = (e) => { const d = drag.current; if (!d) return; update((x) => { x.nodes = x.nodes.map((n) => (n.id === d.id ? { ...n, x: Math.max(0, e.clientX - d.dx), y: Math.max(0, e.clientY - d.dy) } : n)); return x; }); };
  const up = () => { drag.current = null; };
  const W = 200, H = 46;
  const pos = (id) => sc.nodes.find((n) => n.id === id);
  const tryPlan = async (ev) => { try { setPlan({ ev, acts: await api('/api/scenarios/plan/' + ev, null, 'POST') }); } catch (e) { /* shown */ } };
  const field = (k, label, type) => html`<label class="field">${label}<input type=${type || 'text'} value=${node[k]} onInput=${(e) => setField(k, type === 'number' ? +e.target.value : e.target.value)} /></label>`;
  return html`<${Web} path="/admin/studio" studio=${settings.studio}>
    <div class="row" style="justify-content:space-between"><div><h1 class="w-title">Le studio en nœuds</h1><span class="w-sub">Un déclencheur, des conditions, des actions. Glissez les nœuds ; reliez une sortie (à droite) à une entrée (à gauche).</span></div>
      <div class="row"><select class="winput" style="width:220px" value=${cur} onChange=${(e) => { setCur(+e.target.value); setSel(null); }}>${scs.map((x, i) => html`<option value=${i}>${x.name}</option>`)}</select>
        <label class="row w-sub" style="gap:6px"><input type="checkbox" checked=${sc.enabled !== false} onChange=${(e) => update((x) => ({ ...x, enabled: e.target.checked }))} />actif</label>
        <button type="button" class="wbtn" onClick=${() => { const id = 's' + Date.now().toString(36); setSettings({ ...settings, scenarios: [...scs, { id, name: 'Nouveau scénario', enabled: true, nodes: [], wires: [] }] }); setCur(scs.length); }}>Nouveau</button>
        <button type="button" class="wbtn pri" onClick=${save}>Enregistrer</button></div></div>
    <div class="nodes-layout">
      <div class="box palette">${PALETTE.map(([t, items]) => html`<small>${t.toUpperCase()}</small>${items.map((it) => html`<button type="button" onClick=${() => add(it)}><i style=${{ background: KINDS[kindOf(it)].color }}></i>${nodeText(it)}</button>`)}`)}
        <small>APPAREILS</small>${sys ? Object.entries(sys.devices).map(([k, v]) => html`<span class="w-sub" style="font-size:12px">${Array.isArray(v) ? v.length + ' caméras' : v}</span>`) : null}</div>
      <div class="canvas" onPointerMove=${move} onPointerUp=${up} onClick=${(e) => { if (e.target.classList.contains('canvas')) { setSel(null); setArm(null); } }}>
        <svg width="1400" height="900" aria-hidden="true">${sc.wires.map((w) => { const a = pos(w.from), b = pos(w.to); if (!a || !b) return null; const x1 = a.x + W, y1 = a.y + H / 2, x2 = b.x, y2 = b.y + H / 2; const mx = (x1 + x2) / 2;
          return html`<path d=${`M${x1},${y1} C${mx},${y1} ${mx},${y2} ${x2},${y2}`} fill="none" stroke=${KINDS[kindOf(b)].color} stroke-width="2" opacity=".75" />`; })}</svg>
        ${sc.nodes.map((n) => { const k = KINDS[kindOf(n)]; return html`<div class=${'node' + (sel === n.id ? ' sel' : '')} style=${{ left: n.x + 'px', top: n.y + 'px' }} onPointerDown=${(e) => down(e, n)}>
          ${kindOf(n) !== 'trigger' ? html`<span class="port in" title="entrée" onClick=${() => port(n.id, false)}></span>` : null}
          <small style=${{ color: k.color }}>${k.tag.toUpperCase()}</small>${nodeText(n)}
          <span class=${'port out' + (arm === n.id ? ' armed' : '')} title="sortie" onClick=${() => port(n.id, true)}></span></div>`; })}
      </div>
      <div class="box">
        <h2>${node ? 'Nœud sélectionné' : 'Essayer à blanc'}</h2>
        ${node ? html`<span class="tag">${KINDS[kindOf(node)].tag}</span><b>${nodeText(node)}</b>
          ${node.type === 'trigger' ? html`<label class="field">Quand<select value=${node.on} onChange=${(e) => setField('on', e.target.value)}>${Object.entries(EVENTS).map(([k, v]) => html`<option value=${k}>${v}</option>`)}</select></label>` : null}
          ${node.type === 'press' ? html`${field('label', 'Nom')}${field('page', 'Page', 'number')}${field('row', 'Ligne', 'number')}${field('column', 'Colonne', 'number')}` : null}
          ${node.type === 'variable' ? html`${field('name', 'Variable')}${field('value', 'Valeur')}` : null}
          ${node.type === 'route' ? html`${field('source', 'Source (PGM = programme)')}${field('destination', 'Destination ({GO} = GO choisi)')}` : null}
          ${node.type === 'cameraPreset' ? html`${field('camera', 'Caméra', 'number')}${field('preset', 'Preset', 'number')}` : null}
          ${node.type === 'ifSeat' ? field('seat', 'Place', 'number') : null}
          ${node.type === 'wait' ? field('ms', 'Millisecondes', 'number') : null}
          ${['camerasPower', 'mics'].includes(node.type) ? html`<label class="row w-sub" style="gap:6px"><input type="checkbox" checked=${node.on} onChange=${(e) => setField('on', e.target.checked)} />allumer / ouvrir</label>` : null}
          <div class="row"><button type="button" class="wbtn danger" onClick=${del}>Supprimer le nœud</button></div>` : html`
          <span class="w-sub">Ce que chaque moment déclencherait, sans rien envoyer.</span>
          <div class="row">${Object.entries(EVENTS).map(([k, v]) => html`<button type="button" class="wbtn" onClick=${() => tryPlan(k)}>${v}</button>`)}</div>
          ${plan ? html`<b>${EVENTS[plan.ev]}</b>${plan.acts.length ? html`<ol style="margin:0;padding-left:18px;font-size:13px">${plan.acts.map((a) => html`<li>${nodeText(a)}</li>`)}</ol>` : html`<span class="w-sub">rien</span>`}
            <button type="button" class="wbtn" onClick=${() => post('/api/scenarios/run/' + plan.ev).then(() => toast('Envoyé aux appareils'))}>L’envoyer pour de vrai</button>` : null}`}
      </div>
    </div>
  <//>`;
}

// ---------- the administration: directing ----------

const KNOBS = [
  ['Rythme', 'Combien de temps un plan tient, ce qui compte comme une prise de parole.', [['minShotMs', 'Plan minimum', 1000, 8000, 500, 's'], ['maxShotMs', 'Plan maximum', 6000, 30000, 1000, 's'], ['speechOnsetMs', 'Une parole compte après', 200, 2000, 100, 's']]],
  ['Respiration', 'Le plan large revient régulièrement, et à chaque relance de l’animateur.', [['breatheEveryMs', 'Un plan large au moins toutes les', 15000, 120000, 5000, 's'], ['silenceToWideMs', 'Plan large après un silence de', 500, 6000, 250, 's']]],
  ['Plans d’écoute', 'Sur une longue prise de parole, quelqu’un qui écoute.', [['reactionAfterMs', 'Après une parole de', 4000, 30000, 1000, 's'], ['reactionMs', 'Durée du plan d’écoute', 1000, 6000, 500, 's']]],
  ['Vie et suivi', 'Zoom très lent sur un gros plan (jamais en 9:16), caméra qui suit la personne.', [['driftAfterMs', 'Zoom lent après', 1000, 10000, 500, 's'], ['driftMs', 'Durée du zoom lent', 4000, 30000, 1000, 's'], ['followTolerance', 'Le visage peut bouger de', 0.02, 0.2, 0.01, '%']]],
  ['Écoute de l’animateur', 'Une intention dure un temps, une source extérieure passe en grand après un temps.', [['cueLifeMs', 'Oublier une intention après', 3000, 60000, 1000, 's'], ['remoteFullMs', 'Source extérieure en grand après', 2000, 20000, 1000, 's'], ['phraseEndMs', 'Fin de phrase après un silence de', 150, 1000, 50, 's']]],
  ['Reprise en main', 'Une coupe au pupitre suspend l’IA pendant ce temps au moins.', [['humanCutGraceMs', 'Rendre la main à l’IA après', 5000, 300000, 5000, 's']]],
];
const show = (v, unit) => (unit === '%' ? Math.round(v * 100) + ' %' : (v / 1000).toFixed(v % 1000 ? 1 : 0).replace('.', ',') + ' s');

function Directing() {
  const [settings, , setSettings] = useFetch('/api/settings');
  const [shoots] = useFetch('/api/tournages');
  const [cur, setCur] = useState(0);
  const [shootId, setShootId] = useState('');
  const [rep, setRep] = useState(null);
  const [busy, setBusy] = useState(false);
  if (!settings) return html`<${Web} path="/admin/realisation"><p class="w-sub">Chargement…</p><//>`;
  const profiles = settings.profiles;
  const p = profiles[cur];
  const setP = (k, v) => setSettings({ ...settings, profiles: profiles.map((x, i) => (i === cur ? { ...x, [k]: v } : x)) });
  const save = async () => { try { await api('/api/profiles', profiles, 'PUT'); toast('Réglage enregistré'); } catch (e) { /* shown */ } };
  const replay = async () => {
    const id = shootId || (shoots && shoots[0] && shoots[0].id);
    if (!id) { toast('Aucun tournage à rejouer'); return; }
    setBusy(true);
    try { setRep(await api(`/api/tournages/${id}/replay`, { profile: p }, 'POST')); } catch (e) { setRep(null); }
    setBusy(false);
  };
  const ST = [['cutsPerMinute', 'Coupes par minute', (v) => v.toFixed(1).replace('.', ',')], ['meanShotS', 'Plan moyen', (v) => v.toFixed(0) + ' s'], ['widePercent', 'Part des plans larges', (v) => v.toFixed(0) + ' %'], ['listeningShots', 'Plans d’écoute', (v) => v]];
  return html`<${Web} path="/admin/realisation" studio=${settings.studio}>
    <div class="row" style="justify-content:space-between"><h1 class="w-title">Réalisation par l’IA</h1>
      <div class="row"><div class="w-nav">${profiles.map((x, i) => html`<a href="#" class=${i === cur ? 'on' : null} onClick=${(e) => { e.preventDefault(); setCur(i); }}>${x.name}</a>`)}</div><button type="button" class="wbtn pri" onClick=${save}>Enregistrer</button></div></div>
    <div class="cols">
      <div class="stack" style="flex:1 1 520px;display:flex;flex-direction:column;gap:14px">${KNOBS.map(([t, hint, rows]) => html`<div class="box"><h2>${t}</h2><span class="w-sub">${hint}</span>
        ${rows.map(([k, label, min, max, step, unit]) => html`<label class="slider"><span>${label}</span><input type="range" min=${min} max=${max} step=${step} value=${p[k]} onInput=${(e) => setP(k, +e.target.value)} /><span>${show(p[k], unit)}</span></label>`)}</div>`)}</div>
      <div style="flex:1 1 480px;display:flex;flex-direction:column;gap:14px">
        <div class="box"><h2>Rejouer un tournage avec ce réglage</h2>
          <span class="w-sub">L’IA rejoue ce que Studio a entendu et vu pendant le tournage, sans rien envoyer aux appareils. Mêmes entrées : seul le réglage change.</span>
          <div class="row"><select class="winput" style="flex:1" aria-label="Tournage à rejouer" value=${shootId || (shoots && shoots[0] ? shoots[0].id : '')} onChange=${(e) => setShootId(e.target.value)}>${(shoots || []).map((r) => html`<option value=${r.id}>${r.programme} · ${dateFr(r.startedUnixMs)}</option>`)}</select>
            <button type="button" class="wbtn pri" disabled=${busy} onClick=${replay}>${busy ? 'Calcul…' : 'Rejouer'}</button></div>
          ${rep ? html`<span class="w-sub">Réglage du tournage</span><${Strip} segments=${rep.current.segments} duration=${rep.durationMs} />
            <span class="w-sub">Ce réglage</span><${Strip} segments=${rep.proposed.segments} duration=${rep.durationMs} />
            <table class="wt"><thead><tr><th>sur ${fmt(rep.durationMs / 1000)}</th><th>Tournage</th><th>Ce réglage</th></tr></thead><tbody>${ST.map(([k, t, f]) => html`<tr><td>${t}</td><td class="mono">${f(rep.current.stats[k])}</td><td class="mono" style="color:#bfcaff">${f(rep.proposed.stats[k])}</td></tr>`)}</tbody></table>` : (shoots && !shoots.length ? html`<span class="w-sub">Aucun tournage enregistré pour l’instant.</span>` : '')}
        </div>
        <div class="box"><h2>Mode ombre</h2><span class="w-sub">Quand un réalisateur tient le pupitre, l’IA décide sans rien envoyer, et l’on compare. Passez en ombre depuis la page Direct.</span>
          ${(() => { const withAg = (shoots || []).filter((r) => r.agreement && r.agreement.humanCuts); if (!withAg.length) return html`<span class="w-sub">Pas encore de tournage en mode ombre.</span>`;
            const m = withAg.reduce((a, r) => a + r.agreement.matched, 0), hc = withAg.reduce((a, r) => a + r.agreement.humanCuts, 0); const pct = Math.round(m * 100 / hc);
            return html`<div class="row" style="align-items:baseline"><span style="font-size:32px;font-weight:800">${pct} %</span><span>des coupes du réalisateur suivies par l’IA à moins d’une seconde · ${withAg.length} tournage${withAg.length > 1 ? 's' : null}</span></div><div class="bar" style="background:#1f2331"><i style=${{ width: pct + '%', background: '#22c55e' }}></i></div>`; })()}
        </div>
      </div>
    </div>
  <//>`;
}

// ---------- the administration: system ----------

function System() {
  const [s, , setS] = useFetch('/api/settings');
  const [sys] = useFetch('/api/system');
  const [restart, setRestart] = useState(false);
  if (!s) return html`<${Web} path="/admin/systeme"><p class="w-sub">Chargement…</p><//>`;
  const at = (path, v) => { const next = JSON.parse(JSON.stringify(s)); let o = next; const k = path.split('.'); for (let i = 0; i < k.length - 1; i++) o = o[k[i]]; o[k[k.length - 1]] = v; setS(next); };
  const get = (path) => path.split('.').reduce((o, k) => (o == null ? o : o[k]), s);
  const F = (path, label, opts) => { const v = get(path); const o = opts || {};
    if (o.choices) return html`<label class="field">${label}<select value=${v} onChange=${(e) => at(path, e.target.value)}>${o.choices.map(([k, t]) => html`<option value=${k}>${t}</option>`)}</select></label>`;
    if (o.num) return html`<label class="field">${label}<input type="number" value=${v} onInput=${(e) => at(path, +e.target.value)} /></label>`;
    if (o.secret) return html`<label class="field">${label}<input type="password" autocomplete="off" value=${v} onInput=${(e) => at(path, e.target.value)} /></label>`;
    if (o.list) return html`<label class="field">${label}<input value=${(v || []).join(o.sep || ' ')} onInput=${(e) => at(path, e.target.value.split(o.sep || ' ').filter((x) => x !== '').map(o.map || ((x) => x)))} /></label>`;
    return html`<label class="field">${label}<input value=${v == null ? '' : v} placeholder=${o.ph || ''} onInput=${(e) => at(path, e.target.value)} /></label>`; };
  const save = async () => { try { const r = await api('/api/settings', s, 'PUT'); setRestart(!!r.restart); toast(r.restart ? 'Enregistré · redémarrez pour changer d’appareils' : 'Enregistré'); } catch (e) { /* shown */ } };
  const reboot = async () => { try { await api('/api/system/restart', null, 'POST'); toast('Studio redémarre…'); setTimeout(() => location.reload(), 4000); } catch (e) { /* shown */ } };
  return html`<${Web} path="/admin/systeme" studio=${s.studio}>
    <div class="row" style="justify-content:space-between"><div><h1 class="w-title">Système</h1><span class="w-sub">Une adresse laissée vide : l’appareil est simulé. Les appareils changent au redémarrage de Studio.</span></div>
      <div class="row">${restart ? html`<button type="button" class="wbtn" onClick=${reboot}>Redémarrer Studio</button>` : null}<button type="button" class="wbtn pri" onClick=${save}>Enregistrer</button></div></div>
    ${sys ? html`<div class="box"><h2>Le serveur</h2><div class="fields w-sub" style="font-size:13px">
      <span>Version ${sys.version} · en marche depuis ${fmt(sys.uptimeS)}</span><span>Réglages : <span class="mono">${sys.settingsFile}</span></span><span>${sys.ffmpeg || 'ffmpeg introuvable'}</span><span>NDI : ${sys.ndi ? 'runtime installé' : 'non installé'}</span>
      <span>Mélangeur : ${sys.devices.switcher} ${sys.devices.switcherConnected ? '· connecté' : '· non connecté'}</span><span>Console : ${sys.devices.console}</span><span>Buttons : ${sys.devices.automation}</span><span>Nodal : ${sys.devices.router}</span><span>Micros : ${sys.devices.microphones}</span>${sys.simulated ? html`<span class="tag warn">studio simulé</span>` : null}</div>
      ${sys.deviceErrors && sys.deviceErrors.length ? html`<div class="stack" style="display:flex;flex-direction:column;gap:6px">${sys.deviceErrors.map(([l, e]) => html`<span><span class="tag live">${l}</span> <span class="w-sub">ne répondait pas au démarrage, simulé en attendant : ${e}</span></span>`)}</div>` : null}</div>` : null}
    <div class="box"><h2>Studio</h2><div class="fields">${F('studio', 'Nom du studio')}</div></div>
    <div class="box"><h2>Mélangeur et console</h2><div class="fields">
      ${F('switcher.kind', 'Mélangeur', { choices: [['sim', 'simulé'], ['atem', 'ATEM'], ['vmix', 'vMix']] })}${F('switcher.address', 'Adresse', { ph: '192.168.10.2' })}${F('switcher.vmixSplit', 'vMix : entrée 50/50', { num: 1 })}${F('switcher.vmixPip', 'vMix : entrée vignette', { num: 1 })}
      ${F('console.kind', 'Console', { choices: [['sim', 'simulée'], ['m32', 'Midas M32'], ['dm5', 'Yamaha DM5']] })}${F('console.address', 'Adresse', { ph: '192.168.10.4' })}</div></div>
    <div class="box"><h2>Caméras</h2><div class="fields">${F('cameras.control', 'Pilotage', { choices: [['sim', 'simulé'], ['visca', 'VISCA over IP']] })}${F('cameras.travelMs', 'Temps d’un preset (ms)', { num: 1 })}
      ${s.setup.cameras.map((c, i) => F(`setup.cameras.${i}.address`, `${c.label} · adresse`))}${s.setup.cameras.map((c, i) => F(`setup.cameras.${i}.switcherInput`, `${c.label} · entrée du mélangeur`, { num: 1 }))}</div></div>
    <div class="box"><h2>Nodal</h2><div class="fields">${F('router.kind', 'Nodal', { choices: [['sim', 'simulé'], ['videohub', 'Videohub (SDI)'], ['nmos', 'NMOS IS-05 (2110)'], ['ndi', 'NDI (routage de Studio)']] })}${F('router.address', 'Adresse du Videohub')}${F('router.pgmSource', 'Source du programme')}${F('router.ndiPrefix', 'NDI : nom des sources de Studio')}</div>
      <span class="w-sub">NMOS : les émetteurs et récepteurs se déclarent dans <span class="mono">reglages.json</span> (nom, adresse de l’API Connection, identifiant).</span></div>
    <div class="box"><h2>Son et écoute</h2><div class="fields">${F('audio.sdp', 'Fichier SDP du flux AES67', { ph: '/etc/strata-studio/console.sdp' })}${F('audio.interface', 'Interface réseau Dante')}${F('audio.whisper', 'Serveur whisper.cpp', { ph: '127.0.0.1:8178' })}</div></div>
    <div class="box"><h2>Bitfocus Buttons et Stream Deck</h2><div class="fields">${F('buttons.address', 'Adresse de Buttons', { ph: '192.168.10.5:8000' })}
      <label class="row w-sub" style="gap:6px"><input type="checkbox" checked=${s.buttons.deck} onChange=${(e) => at('buttons.deck', e.target.checked)} />écrire les touches du Stream Deck dans Buttons</label></div></div>
    <div class="box"><h2>Plateformes GO</h2><div class="fields">${s.gos.map((g, i) => html`${F(`gos.${i}.label`, 'Nom')}${F(`gos.${i}.address`, `${g.label} · adresse du Streaming Encoder`)}${F(`gos.${i}.destination`, `${g.label} · sortie du nodal`)}`)}</div></div>
    <div class="box"><h2>YouTube et TikTok</h2><div class="fields">${F('live.youtube.url', 'YouTube · serveur RTMP')}${F('live.youtube.key', 'YouTube · clé', { secret: 1 })}${F('live.tiktok.url', 'TikTok · serveur')}${F('live.tiktok.key', 'TikTok · clé', { secret: 1 })}
      ${F('live.accel', 'Encodage', { choices: [['cpu', 'processeur'], ['nvenc', 'NVIDIA'], ['qsv', 'Intel Quick Sync']] })}${F('live.input', 'Entrée ffmpeg du programme (vide : celle du PGM)', { list: 1 })}</div></div>
    <div class="box"><h2>Calls (Quicklink StudioEdge)</h2><div class="fields">${F('calls.api', 'Adresse de l’API', { ph: 'https://…' })}${F('calls.username', 'Compte')}${F('calls.password', 'Mot de passe', { secret: 1 })}${F('calls.room', 'Salle de distribution')}
      ${F('calls.positions', 'Positions (call 1, 2…)', { list: 1, map: Number })}${F('calls.inputs', 'Entrées du mélangeur (call 1, 2…)', { list: 1, map: Number })}${F('agencyInput', 'Agence : entrée', { num: 1 })}${F('duplexInput', 'Duplex : entrée', { num: 1 })}</div></div>
    <div class="box"><h2>Stockages (partages montés)</h2><div class="fields">${s.storages.map((st, i) => html`${F(`storages.${i}.label`, 'Nom')}${F(`storages.${i}.path`, 'Dossier', { ph: '/mnt/redaction' })}`)}</div>
      <div class="row"><button type="button" class="wbtn" onClick=${() => at('storages', [...s.storages, { label: 'Nouveau stockage', path: '/mnt/' }])}>Ajouter un stockage</button>${s.storages.length > 1 ? html`<button type="button" class="wbtn danger" onClick=${() => at('storages', s.storages.slice(0, -1))}>Retirer le dernier</button>` : null}</div></div>
    <div class="box"><h2>Enregistrement</h2><div class="fields">${F('recording.codec', 'Codec', { choices: [['h264', 'H.264 4:2:2'], ['h265', 'H.265 4:2:2']] })}${F('recording.accel', 'Encodage', { choices: [['cpu', 'processeur'], ['nvenc', 'NVIDIA'], ['qsv', 'Intel Quick Sync']] })}${F('recording.container', 'Conteneur', { choices: [['mov', 'MOV'], ['mp4', 'MP4']] })}
      <label class="row w-sub" style="gap:6px"><input type="checkbox" checked=${s.recording.light} onChange=${(e) => at('recording.light', e.target.checked)} />images légères (essai sans carte d’encodage)</label></div>
      <div class="fields">${s.recording.channels.map((c, i) => F(`recording.channels.${i}.input`, `${c.name} · entrée ffmpeg`, { list: 1 }))}</div></div>
  <//>`;
}

// ---------- routes ----------

function App() {
  const p = location.pathname.replace(/\/+$/, '') || '/';
  if (p === '/') return html`<${Journey} />`;
  if (p === '/ecran') return html`<${Screen} />`;
  if (p === '/deck') return html`<${Deck} />`;
  if (p === '/tournages') return html`<${Shoots} />`;
  if (p.startsWith('/tournages/')) return html`<${Shoot} id=${decodeURIComponent(p.slice(11))} />`;
  if (p === '/direct') return html`<${Live} />`;
  if (p === '/programmes') return html`<${Programmes} />`;
  if (p === '/admin/studio') return html`<${Nodes} />`;
  if (p === '/admin/realisation') return html`<${Directing} />`;
  if (p === '/admin' || p === '/admin/systeme') return html`<${System} />`;
  return html`<${Journey} />`;
}

if (['/ecran', '/deck'].includes(location.pathname) || location.pathname.startsWith('/tournages') || location.pathname.startsWith('/admin') || ['/direct', '/programmes'].includes(location.pathname)) document.body.style.background = '#0b0c12';
render(html`<${App} />`, document.getElementById('app'));
