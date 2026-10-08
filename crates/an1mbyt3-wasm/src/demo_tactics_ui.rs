//! Sunset Tactics' screens as HTML + CSS (`UiHtml` nodes): the agent-select
//! menu and the in-match HUD. Authored at 1600×900 CSS px and placed at a
//! node scale of 0.25, so they cover the 400×225 view exactly and stay
//! crisp at any window size.
//!
//! Look: an original tactical-shooter style — condensed DIN-like type
//! (Bahnschrift ships with Windows; narrow sans fallbacks elsewhere), angled
//! translucent panels, your team in teal, the enemy in red. No game art is
//! used; every icon is CSS.
//!
//! The match script fills them in through `ref('hud').dom` /
//! `ref('selectUi').dom` (`demo_tactics.js`), touching the DOM only when a
//! value changes.

/// The in-match HUD (non-interactive: the mouse aims through it).
pub(crate) const HUD_HTML: &str = r#"<div class="hud">
  <div class="top">
    <div class="team us">
      <div class="pips" id="pipsA"><i></i><i></i><i></i><i></i><i></i></div>
      <div class="side" id="sideA">ATK</div>
      <div class="sc" id="scoreA">0</div>
    </div>
    <div class="clock" id="clock"><div class="time" id="time">1:40</div><div class="phase" id="phase">BUY PHASE</div></div>
    <div class="team them">
      <div class="sc" id="scoreB">0</div>
      <div class="side" id="sideB">DEF</div>
      <div class="pips" id="pipsB"><i></i><i></i><i></i><i></i><i></i></div>
    </div>
  </div>
  <div class="spike" id="spike"><span class="dot"></span><span id="spikeText"></span></div>
  <div class="banner" id="banner"><div class="msg" id="msg"></div><div class="sub" id="sub"></div></div>
  <div class="feed" id="feed"><div class="row"></div><div class="row"></div><div class="row"></div><div class="row"></div></div>
  <div class="prog" id="prog"><div class="label" id="progLabel"></div><div class="bar"><div class="fill" id="progFill"></div></div></div>
  <div class="help" id="help"></div>
  <div class="vitals">
    <div class="agent" id="agent">JETT</div>
    <div class="hprow">
      <div class="armor"><span class="shield"></span><span id="armor">50</span></div>
      <div class="hp" id="hp">100</div>
    </div>
    <div class="hpbar"><div class="fill" id="hpFill"></div></div>
  </div>
  <div class="abilities">
    <div class="ab" id="ab0"><div class="icon"><span class="key">C</span></div><div class="name"></div><div class="charges"></div></div>
    <div class="ab" id="ab1"><div class="icon"><span class="key">Q</span></div><div class="name"></div><div class="charges"></div></div>
    <div class="ab" id="ab2"><div class="icon"><span class="key">E</span></div><div class="name"></div><div class="charges"></div></div>
    <div class="ab ult" id="ab3"><div class="icon"><span class="key">X</span></div><div class="name"></div><div class="charges"></div></div>
  </div>
  <div class="ammo">
    <div class="mag" id="mag">25</div>
    <div class="reserve" id="reserve">/ 25</div>
    <div class="gun" id="gun">RIFLE</div>
  </div>
</div>"#;

pub(crate) const HUD_CSS: &str = r#"
* { box-sizing: border-box; margin: 0; padding: 0; }
.hud { position: relative; width: 1600px; height: 900px; color: #ece8e1; font-family: Bahnschrift, 'DIN Condensed', 'Barlow Condensed', 'Arial Narrow', 'Roboto Condensed', sans-serif; font-stretch: condensed; letter-spacing: 0.04em; text-shadow: 0 2px 6px rgba(0,0,0,0.55); }
.top { position: absolute; top: 22px; left: 50%; transform: translateX(-50%); display: flex; align-items: stretch; gap: 6px; }
.team { display: flex; align-items: center; gap: 12px; padding: 0 18px; height: 64px; background: rgba(12,16,22,0.62); }
.team.us { clip-path: polygon(14px 0, 100% 0, 100% 100%, 0 100%); }
.team.them { clip-path: polygon(0 0, calc(100% - 14px) 0, 100% 100%, 0 100%); }
.sc { font-size: 48px; font-weight: 700; min-width: 40px; text-align: center; }
.us .sc { color: #5be0c3; }
.them .sc { color: #ff4655; }
.side { font-size: 18px; font-weight: 600; opacity: 0.75; letter-spacing: 0.18em; }
.pips { display: flex; gap: 5px; }
.pips i { width: 11px; height: 24px; transform: skewX(-14deg); background: rgba(255,255,255,0.14); }
.us .pips i.on { background: #5be0c3; box-shadow: 0 0 10px rgba(91,224,195,0.6); }
.them .pips i.on { background: #ff4655; box-shadow: 0 0 10px rgba(255,70,85,0.6); }
.clock { min-width: 150px; height: 64px; display: flex; flex-direction: column; align-items: center; justify-content: center; background: rgba(12,16,22,0.78); border-bottom: 3px solid rgba(236,232,225,0.25); }
.time { font-size: 36px; font-weight: 700; line-height: 1; }
.phase { font-size: 13px; font-weight: 600; letter-spacing: 0.24em; opacity: 0.7; margin-top: 3px; }
.clock.planted { border-bottom-color: #ff4655; }
.clock.planted .time { color: #ff4655; animation: pulse 0.8s ease-in-out infinite alternate; }
@keyframes pulse { from { opacity: 1; } to { opacity: 0.55; } }

.spike { position: absolute; top: 100px; left: 50%; transform: translateX(-50%); display: none; align-items: center; gap: 10px; padding: 6px 16px; background: rgba(255,70,85,0.18); border: 1px solid rgba(255,70,85,0.55); font-size: 17px; font-weight: 600; letter-spacing: 0.2em; color: #ffb3ba; }
.spike.on { display: flex; }
.spike .dot { width: 10px; height: 10px; background: #ff4655; transform: rotate(45deg); }

.banner { position: absolute; top: 230px; left: 0; right: 0; text-align: center; }
.msg { font-size: 72px; font-weight: 700; letter-spacing: 0.12em; }
.msg.win { color: #5be0c3; }
.msg.lose { color: #ff4655; }
.sub { font-size: 24px; font-weight: 600; letter-spacing: 0.3em; opacity: 0.85; margin-top: 6px; }

.feed { position: absolute; top: 24px; right: 28px; display: flex; flex-direction: column; align-items: flex-end; gap: 6px; }
.feed .row { display: none; font-size: 18px; font-weight: 600; padding: 6px 14px 6px 12px; background: rgba(12,16,22,0.62); border-right: 4px solid #ff4655; letter-spacing: 0.08em; }
.feed .row.on { display: block; }
.feed .row.mine { border-right-color: #5be0c3; }

.prog { position: absolute; top: 560px; left: 50%; transform: translateX(-50%); width: 360px; display: none; text-align: center; }
.prog.on { display: block; }
.prog .label { font-size: 18px; font-weight: 600; letter-spacing: 0.3em; margin-bottom: 8px; }
.prog .bar { height: 8px; background: rgba(0,0,0,0.55); border: 1px solid rgba(255,255,255,0.25); }
.prog .fill { height: 100%; width: 0; background: #ff4655; }

.help { position: absolute; bottom: 150px; left: 0; right: 0; text-align: center; font-size: 17px; font-weight: 600; letter-spacing: 0.22em; opacity: 0.8; }

.vitals { position: absolute; left: 40px; bottom: 34px; width: 300px; padding: 12px 18px 16px; background: linear-gradient(90deg, rgba(12,16,22,0.72), rgba(12,16,22,0.2)); }
.agent { font-size: 16px; font-weight: 600; letter-spacing: 0.3em; opacity: 0.75; }
.hprow { display: flex; align-items: flex-end; gap: 16px; }
.hp { font-size: 64px; font-weight: 700; line-height: 1; }
.hp.low { color: #ff4655; }
.armor { display: flex; align-items: center; gap: 6px; font-size: 24px; font-weight: 600; color: #9fd8ff; padding-bottom: 8px; }
.armor.none { opacity: 0.3; }
.shield { width: 16px; height: 19px; background: #9fd8ff; clip-path: polygon(50% 0, 100% 18%, 100% 55%, 50% 100%, 0 55%, 0 18%); }
.hpbar { margin-top: 8px; height: 5px; background: rgba(255,255,255,0.15); }
.hpbar .fill { height: 100%; width: 100%; background: #ece8e1; }
.hpbar .fill.low { background: #ff4655; }

.abilities { position: absolute; bottom: 34px; left: 50%; transform: translateX(-50%); display: flex; gap: 12px; }
.ab { width: 104px; display: flex; flex-direction: column; align-items: center; gap: 5px; opacity: 0.45; }
.ab.ready { opacity: 1; }
.ab .icon { position: relative; width: 70px; height: 70px; background: rgba(12,16,22,0.7); border: 2px solid rgba(236,232,225,0.35); display: flex; align-items: center; justify-content: center; clip-path: polygon(12px 0, 100% 0, 100% calc(100% - 12px), calc(100% - 12px) 100%, 0 100%, 0 12px); }
.ab.ready .icon { border-color: #ece8e1; }
.ab .key { font-size: 30px; font-weight: 700; }
.ab .name { font-size: 12px; font-weight: 600; letter-spacing: 0.12em; white-space: nowrap; opacity: 0.8; }
.ab .charges { display: flex; gap: 4px; height: 6px; }
.ab .charges i { width: 14px; height: 6px; background: #ece8e1; transform: skewX(-20deg); }
.ab .charges i.off { background: rgba(255,255,255,0.18); }
.ab.ult .icon { border-color: rgba(255,214,102,0.45); }
.ab.ult.ready .icon { border-color: #ffd666; box-shadow: inset 0 0 18px rgba(255,214,102,0.45); }
.ab.ult.ready .key { color: #ffd666; }
.ab.ult .charges i { width: 8px; height: 8px; transform: rotate(45deg); background: rgba(255,214,102,0.2); }
.ab.ult .charges i.on { background: #ffd666; }

.ammo { position: absolute; right: 40px; bottom: 34px; width: 260px; padding: 12px 18px 16px; text-align: right; background: linear-gradient(270deg, rgba(12,16,22,0.72), rgba(12,16,22,0.2)); }
.mag { display: inline-block; font-size: 64px; font-weight: 700; line-height: 1; }
.mag.low { color: #ff4655; }
.reserve { display: inline-block; font-size: 26px; font-weight: 600; opacity: 0.6; margin-left: 6px; }
.gun { font-size: 15px; font-weight: 600; letter-spacing: 0.3em; opacity: 0.7; margin-top: 4px; }
"#;

/// The agent-select menu (interactive).
pub(crate) const SELECT_HTML: &str = r#"<div class="sel">
  <header>
    <div class="brand"><span class="mark"></span><h1>SUNSET TACTICS</h1></div>
    <div class="rec" id="rec">0W 0L</div>
  </header>
  <h2>SELECT YOUR AGENT</h2>
  <div class="agents" id="agents"></div>
  <section class="kit">
    <div class="who"><div class="name" id="kitName"></div><div class="role" id="kitRole"></div></div>
    <ul id="kitList"></ul>
  </section>
  <section class="ai">
    <div class="ai-head"><h3>AI DIFFICULTY</h3><div class="hint" id="hint"></div></div>
    <div class="seg" id="diff"></div>
    <div class="rows" id="rows"></div>
  </section>
  <footer>
    <button class="ghost" id="viewBtn">VIEW: 3D</button>
    <button class="lock" id="lock">LOCK IN</button>
  </footer>
</div>"#;

pub(crate) const SELECT_CSS: &str = r#"
* { box-sizing: border-box; margin: 0; padding: 0; }
button { font: inherit; color: inherit; background: none; border: none; cursor: pointer; letter-spacing: inherit; }
.sel { position: relative; width: 1600px; height: 900px; padding: 46px 90px; color: #ece8e1; font-family: Bahnschrift, 'DIN Condensed', 'Barlow Condensed', 'Arial Narrow', 'Roboto Condensed', sans-serif; font-stretch: condensed; letter-spacing: 0.05em;
  background: radial-gradient(1200px 600px at 75% 15%, rgba(255,70,85,0.16), transparent 60%), radial-gradient(900px 500px at 10% 100%, rgba(91,224,195,0.12), transparent 60%), linear-gradient(160deg, #10151d 0%, #0b0e14 100%); }
header { display: flex; align-items: center; justify-content: space-between; }
.brand { display: flex; align-items: center; gap: 16px; }
.mark { width: 22px; height: 22px; background: #ff4655; transform: rotate(45deg); }
h1 { font-size: 44px; font-weight: 700; letter-spacing: 0.16em; }
.rec { font-size: 22px; font-weight: 600; letter-spacing: 0.2em; opacity: 0.6; }
h2 { margin: 34px 0 18px; font-size: 18px; font-weight: 600; letter-spacing: 0.4em; opacity: 0.6; }
.agents { display: flex; gap: 18px; }
.agent { position: relative; width: 236px; height: 150px; padding: 18px 20px; text-align: left; background: rgba(255,255,255,0.04); border: 2px solid rgba(255,255,255,0.1); clip-path: polygon(0 0, calc(100% - 22px) 0, 100% 22px, 100% 100%, 0 100%); transition: background 120ms, border-color 120ms, transform 120ms; overflow: hidden; }
.agent::before { content: ''; position: absolute; right: -30px; bottom: -30px; width: 130px; height: 130px; background: var(--c); opacity: 0.18; transform: rotate(45deg); transition: opacity 120ms; }
.agent:hover { background: rgba(255,255,255,0.08); transform: translateY(-3px); }
.agent.on { border-color: var(--c); background: rgba(255,255,255,0.09); }
.agent.on::before { opacity: 0.4; }
.agent .glyph { width: 30px; height: 30px; background: var(--c); transform: rotate(45deg); margin: 6px 0 18px 6px; box-shadow: 0 0 22px var(--c); }
.agent .nm { display: block; font-size: 32px; font-weight: 700; letter-spacing: 0.1em; }
.agent .rl { display: block; font-size: 14px; font-weight: 600; letter-spacing: 0.3em; opacity: 0.6; }
.kit { margin-top: 30px; display: flex; gap: 46px; align-items: flex-start; }
.who { min-width: 240px; }
.who .name { font-size: 58px; font-weight: 700; letter-spacing: 0.1em; line-height: 1; }
.who .role { font-size: 18px; font-weight: 600; letter-spacing: 0.4em; opacity: 0.6; margin-top: 8px; }
.kit ul { list-style: none; display: grid; grid-template-columns: 1fr 1fr; gap: 12px 30px; flex: 1; }
.kit li { display: flex; gap: 14px; align-items: center; padding: 10px 14px; background: rgba(255,255,255,0.04); border-left: 3px solid rgba(255,255,255,0.18); }
.kit li b { flex: none; width: 38px; height: 38px; display: flex; align-items: center; justify-content: center; font-size: 20px; background: rgba(255,255,255,0.08); }
.kit li.ult { border-left-color: #ffd666; }
.kit li.ult b { color: #ffd666; }
.kit li span { font-size: 17px; font-weight: 600; letter-spacing: 0.06em; }
.ai { margin-top: 30px; padding-top: 22px; border-top: 1px solid rgba(255,255,255,0.1); display: grid; grid-template-columns: 340px 1fr; gap: 12px 40px; }
.ai-head h3 { font-size: 22px; font-weight: 700; letter-spacing: 0.25em; }
.hint { font-size: 15px; letter-spacing: 0.18em; opacity: 0.6; margin-top: 6px; }
.seg { display: flex; gap: 6px; align-self: start; }
.seg button { padding: 11px 26px; font-size: 18px; font-weight: 600; letter-spacing: 0.2em; background: rgba(255,255,255,0.05); border: 1px solid rgba(255,255,255,0.12); }
.seg button:hover { background: rgba(255,255,255,0.1); }
.seg button.on { background: #ece8e1; color: #0f1419; border-color: #ece8e1; }
.rows { grid-column: 2; display: grid; grid-template-columns: 1fr 1fr; gap: 10px 40px; }
.row { display: flex; align-items: center; gap: 12px; font-size: 16px; font-weight: 600; letter-spacing: 0.16em; }
.row .lbl { flex: 1; opacity: 0.75; }
.row button { width: 32px; height: 32px; font-size: 22px; background: rgba(255,255,255,0.06); border: 1px solid rgba(255,255,255,0.14); }
.row button:hover { background: rgba(255,255,255,0.14); }
.row .val { width: 64px; text-align: center; font-size: 19px; }
.row .val.up { color: #ff8a93; }
.row .val.down { color: #8fdcff; }
footer { position: absolute; left: 90px; right: 90px; bottom: 44px; display: flex; justify-content: space-between; align-items: center; }
.ghost { padding: 14px 30px; font-size: 18px; font-weight: 600; letter-spacing: 0.25em; border: 1px solid rgba(255,255,255,0.25); }
.ghost:hover { background: rgba(255,255,255,0.08); }
.lock { padding: 18px 70px; font-size: 26px; font-weight: 700; letter-spacing: 0.3em; background: #ff4655; color: #fff; clip-path: polygon(16px 0, 100% 0, calc(100% - 16px) 100%, 0 100%); transition: filter 120ms, transform 120ms; }
.lock:hover { filter: brightness(1.12); transform: translateY(-2px); }
"#;
