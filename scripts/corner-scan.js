// Layout check: opens every DeskZero page, tab, Settings section and sheet and lists
// elements whose rounded corners eat into their text: a pill radius on a box taller
// than one line, or text that starts inside the corner curve. Run it in the browser
// preview (npm run build, serve dist/ with a stand-in Tauri bridge) by evaluating
// this file in the page, e.g. over Chrome DevTools: it returns { found: [...],
// missing: [...] }. Empty `found` and `missing` = pass. Same rules as DeskFolio's.
// Set window.__OLD = true first to prove it still catches a tall pill.
(async () => {
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const press = (el) => { for (const t of ['mousedown', 'mouseup', 'click']) el.dispatchEvent(new MouseEvent(t, { bubbles: true })); };
  const byText = (sel, label) => [...document.querySelectorAll(sel)].find((b) => b.textContent.trim().startsWith(label) || b.title === label);
  const escape = async () => { document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); await wait(250); };
  async function click(sel, label) {
    const el = byText(sel, label);
    if (!el) return false;
    press(el); await wait(600);
    return true;
  }
  async function menu(label) {
    const burger = document.querySelector('.menu-burger');
    if (!burger) return false;
    press(burger); await wait(250);
    // By name: View has a Theme submenu inside it, so positions shift while walking.
    const name = (b) => b.querySelector('.menu-label')?.textContent.trim();
    const tops = [...document.querySelectorAll('.menu-row.has-sub')].map(name);
    for (const t of tops) {
      const top = [...document.querySelectorAll('.menu-row.has-sub')].find((b) => name(b) === t);
      if (!top) continue;
      top.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }));
      press(top); await wait(200);
      const hit = [...document.querySelectorAll('button.menu-row')].find((b) => b.querySelector('.menu-label')?.textContent.trim() === label);
      if (hit) { press(hit); await wait(700); return true; }
    }
    press(burger); await wait(150);
    return false;
  }
  function scan(where) {
    const out = [];
    for (const el of document.querySelectorAll('body *')) {
      const cs = getComputedStyle(el);
      const r = el.getBoundingClientRect();
      if (!r.width || !r.height || cs.visibility === 'hidden' || cs.display === 'none') continue;
      const rad = parseFloat(cs.borderTopLeftRadius) || 0;
      const ownText = [...el.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim());
      if (rad < 8 || !(ownText || el.matches('button, a, [role=button]'))) continue;
      // Folder tiles and picture previews are pictures, not text boxes.
      if (el.closest('.bz-folder, .bz-tile, .bz-thumb')) continue;
      // Buttons count even when see-through: hovering gives them a background.
      const bg = el.matches('button, [role=button]') || cs.backgroundColor !== 'rgba(0, 0, 0, 0)' || parseFloat(cs.borderTopWidth) > 0 || cs.boxShadow !== 'none';
      if (!bg) continue;
      const effective = Math.min(rad, r.height / 2, r.width / 2);
      const padL = parseFloat(cs.paddingLeft) || 0;
      // Tall pill: more than one line in a shape that is round at the ends.
      const tallPill = effective >= r.height / 2 - 1 && r.height > 44 && r.width > r.height * 1.2;
      // Text starting inside the curve: left padding much smaller than the radius on a box taller than a line.
      const tight = r.height > 44 && padL + 2 < effective * 0.45;
      if (tallPill || tight) out.push({ where, tag: el.tagName.toLowerCase(), cls: String(el.className).slice(0, 60), h: Math.round(r.height), radius: Math.round(effective), padL: Math.round(padL), text: el.textContent.trim().slice(0, 40) });
    }
    return out;
  }
  if (window.__OLD) { const st = document.createElement('style'); st.textContent = '.home-drop{border-radius:999px!important;padding:8px 12px!important}'; document.head.append(st); }
  const found = scan('Browser (icons)');
  const missing = [];
  const step = async (where, ok) => { if (await ok()) found.push(...scan(where)); else missing.push(where); };
  await step('Browser (list)', () => click('button', 'List'));
  await step('Clean up sheet', () => click('button', 'Clean up this folder'));
  await escape();
  await step('Organizer: Overview', () => click('.nav-item', 'Organizer'));
  await step('Organize preview', () => click('button', 'Organize now'));
  await escape();
  await step('Organizer: Watch folders', () => click('[role=tab]', 'Watch folders'));
  await step('Organizer: Rules', () => click('[role=tab]', 'Rules'));
  await step('History', () => click('.nav-item', 'History'));
  await step('Settings', () => click('.nav-item', 'Settings'));
  for (const s of ['Editing', 'Features', 'Advanced', 'About']) await step(`Settings: ${s}`, () => click('.settings-nav-item', s));
  for (const m of ['About DeskZero', 'Keyboard shortcuts', 'Search commands…']) { await step(m, () => menu(m)); await escape(); }
  return { found, missing };
})()
