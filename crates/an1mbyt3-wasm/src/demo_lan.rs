//! "LAN Party" — the multiplayer demo: host a game on a port, others join
//! by address (`ctx.net`, relayed by `an1mbyt3-net`).
//!
//! - **Lobby** (an interactive HTML overlay): your name, Host on a port, or
//!   Join an address. While hosting it shows the addresses friends type.
//! - **Arena**: everyone is a toon blob (WASD / arrows). Each player sends
//!   its position 20× a second; others are drawn smoothly interpolated.
//! - **Host-authoritative coins**: the host player owns the coins and the
//!   scores. Others *ask* to grab a coin; the host checks and answers, so
//!   two players can't both get one. If the host leaves, the next player
//!   becomes host and carries on with the coins and scores everyone has.

use an1mbyt3_core::{
    Animation, ArtboardId, CameraNode, Command, Document, FontAsset, LoopMode, Node, NodeBehavior, NodeKind, Path, ProgramDef, ShapeNode, StateDef, StateMachineDef, Stroke,
};
use glam::Vec2;

use crate::demo_ninja::{add_node, text_node, with_id};

const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/PressStart2P-Regular.ttf");
const SCREEN_W: f32 = 960.0;
const SCREEN_H: f32 = 540.0;
/// Avatar and coin pools.
const AVATARS: usize = 8;
const COINS: usize = 5;
const INK: [f32; 4] = [0.12, 0.08, 0.14, 1.0];
/// The arena's half size (players are kept inside it).
const ARENA: Vec2 = Vec2::new(420.0, 210.0);

pub(crate) struct LanDemo {
    pub scene: ArtboardId,
}

fn shape(id: &str, parent: Option<an1mbyt3_core::NodeId>, path: Path, color: [f32; 4], at: Vec2, outline: Option<f32>) -> Node {
    let stroke = outline.map(|width| Stroke { width, color: INK, gradient: None, image: None, cap: Default::default(), join: Default::default() });
    let mut node = with_id(Node::new(id, parent, NodeKind::Shape(ShapeNode { path, fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke })), id);
    node.transform.translation = at;
    node
}

pub(crate) fn build_demo_lan(document: &mut Document) -> LanDemo {
    let mut inverses = Vec::new();
    let font = document.add_font(FontAsset { name: "Press Start 2P".into(), data: FONT_BYTES.to_vec() });
    let inverse = document.apply(Command::AddArtboard { name: "LAN Party".into(), width: SCREEN_W, height: SCREEN_H });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);

    let camera = with_id(
        Node::with_components("Main Camera", None, vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: [0.16, 0.13, 0.22, 1.0], follow_target: None, follow_smoothing: 0.0, vignette: Default::default(), pixel_perfect: Default::default() })]),
        "camera",
    );
    let camera_id = add_node(document, &mut inverses, scene, camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(camera_id) }));

    // The arena floor: a checker of soft tiles inside an outlined border.
    add_node(document, &mut inverses, scene, shape("arena", None, Path::rect(ARENA.x * 2.0 + 40.0, ARENA.y * 2.0 + 40.0), [0.3, 0.26, 0.4, 1.0], Vec2::ZERO, Some(5.0)));
    let tile = 60.0;
    let (cols, rows) = (((ARENA.x * 2.0 + 40.0) / tile) as i32, ((ARENA.y * 2.0 + 40.0) / tile) as i32);
    for r in 0..rows {
        for c in 0..cols {
            if (r + c) % 2 == 0 {
                let at = Vec2::new(-ARENA.x - 20.0 + tile * (c as f32 + 0.5) + ((ARENA.x * 2.0 + 40.0) - cols as f32 * tile) / 2.0, -ARENA.y - 20.0 + tile * (r as f32 + 0.5) + ((ARENA.y * 2.0 + 40.0) - rows as f32 * tile) / 2.0);
                add_node(document, &mut inverses, scene, shape("", None, Path::rect(tile, tile), [0.34, 0.3, 0.45, 1.0], at, None));
            }
        }
    }

    // Coins (the host places them).
    for i in 0..COINS {
        let mut coin = shape(&format!("coin{i}"), None, Path::ellipse(26.0, 26.0), [1.0, 0.82, 0.2, 1.0], Vec2::new(0.0, -2000.0), Some(3.0));
        coin.visible = false;
        let coin_id = add_node(document, &mut inverses, scene, coin);
        add_node(document, &mut inverses, scene, shape(&format!("coinShine{i}"), Some(coin_id), Path::ellipse(7.0, 11.0), [1.0, 0.97, 0.75, 1.0], Vec2::new(-4.0, 3.0), None));
    }

    // Avatars: an outlined blob with eyes, a name tag above.
    for i in 0..AVATARS {
        let mut body = shape(&format!("av{i}"), None, Path::ellipse(46.0, 42.0), [0.5, 0.8, 1.0, 1.0], Vec2::new(0.0, -2000.0), Some(4.0));
        body.visible = false;
        let body_id = add_node(document, &mut inverses, scene, body);
        for (side, x) in [("L", -9.0f32), ("R", 9.0)] {
            let eye = add_node(document, &mut inverses, scene, shape(&format!("av{i}eye{side}"), Some(body_id), Path::ellipse(13.0, 15.0), [1.0, 1.0, 1.0, 1.0], Vec2::new(x, 5.0), Some(2.0)));
            add_node(document, &mut inverses, scene, shape(&format!("av{i}pupil{side}"), Some(eye), Path::ellipse(6.0, 7.0), INK, Vec2::new(1.5, 0.0), None));
        }
        add_node(document, &mut inverses, scene, text_node(Some(body_id), &format!("av{i}name"), "PLAYER", font, 8.0, [1.0, 1.0, 1.0, 1.0], Vec2::new(0.0, 36.0)));
        add_node(document, &mut inverses, scene, text_node(Some(body_id), &format!("av{i}you"), "YOU", font, 7.0, [1.0, 0.85, 0.3, 1.0], Vec2::new(0.0, -34.0)));
    }

    // HUD: room info and scoreboard (HTML, under the camera), and the lobby.
    let mut hud = Node::with_components("HUD", Some(camera_id), vec![NodeBehavior::UiHtml(an1mbyt3_core::UiHtml { html: HUD_HTML.into(), css: HUD_CSS.into(), width: 1600.0, height: 900.0, interactive: false })]);
    hud.transform.scale = Vec2::splat(SCREEN_W / 1600.0);
    add_node(document, &mut inverses, scene, with_id(hud, "hud"));
    let mut lobby = Node::with_components("Lobby", Some(camera_id), vec![NodeBehavior::UiHtml(an1mbyt3_core::UiHtml { html: LOBBY_HTML.into(), css: LOBBY_CSS.into(), width: 1600.0, height: 900.0, interactive: true })]);
    lobby.transform.scale = Vec2::splat(SCREEN_W / 1600.0);
    add_node(document, &mut inverses, scene, with_id(lobby, "lobby"));

    let inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("LAN Party (code)") });
    let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(inverse);
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: "LAN Party".into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: "Play".into(),
                animation,
                loop_mode: LoopMode::Once,
                transitions: Vec::new(),
                position: Vec2::ZERO,
                blend: None,
                program: Some(ProgramDef::Code(format!("const AVATARS = {AVATARS}, COINS = {COINS}, ARENA_X = {}, ARENA_Y = {}\n{SCRIPT}", ARENA.x, ARENA.y))),
            }],
            initial_state: "Play".into(),
            layers: Vec::new(),
            deleted: false,
        },
    }));
    inverses.reverse();
    document.commit_batch(inverses);
    LanDemo { scene }
}

const LOBBY_HTML: &str = r#"<div class="wrap" id="wrap">
  <div class="card">
    <div class="title">LAN PARTY</div>
    <div class="sub">Host a game on a port, or join a friend's by address.</div>
    <label>Your name<input id="name" maxlength="16" value="Player"></label>
    <div class="cols">
      <div class="col">
        <div class="h">Host</div>
        <label>Port<input id="port" value="7777" inputmode="numeric"></label>
        <button id="host">Host game</button>
        <div class="note" id="hostNote">Others on your network join with your address.</div>
      </div>
      <div class="col">
        <div class="h">Join</div>
        <label>Address<input id="addr" value="localhost:7777" placeholder="192.168.1.20:7777"></label>
        <button id="join" class="alt">Join game</button>
        <div class="note">Same machine? Use localhost:7777.</div>
      </div>
    </div>
    <div class="status" id="status"></div>
  </div>
</div>"#;

const LOBBY_CSS: &str = r#"
.wrap { position: absolute; inset: 0; display: grid; place-items: center; background: rgb(14 10 22 / 0.55); font-family: system-ui, 'Segoe UI', sans-serif; color: #f4f0ff; }
.wrap.off { display: none; }
.card { width: 760px; background: #2a2340; border: 4px solid #120c1c; border-radius: 22px; box-shadow: 0 10px 0 #120c1c; padding: 34px 40px 30px; }
.title { font-size: 54px; font-weight: 900; letter-spacing: 3px; color: #ffd23a; text-shadow: 4px 4px 0 #120c1c; }
.sub { font-size: 20px; opacity: 0.8; margin: 6px 0 22px; }
label { display: flex; flex-direction: column; gap: 6px; font-size: 16px; font-weight: 700; text-transform: uppercase; letter-spacing: 1px; opacity: 0.9; }
input { font: inherit; font-size: 22px; font-weight: 600; text-transform: none; letter-spacing: 0; padding: 10px 14px; border-radius: 12px; border: 3px solid #120c1c; background: #f4f0ff; color: #1b1428; outline: none; }
input:focus { border-color: #ffd23a; }
.cols { display: grid; grid-template-columns: 1fr 1fr; gap: 26px; margin-top: 22px; }
.col { display: flex; flex-direction: column; gap: 12px; background: #221c35; border-radius: 16px; padding: 18px; }
.h { font-size: 24px; font-weight: 900; }
button { font: inherit; font-size: 24px; font-weight: 900; padding: 12px; border-radius: 14px; border: 3px solid #120c1c; background: #ffd23a; color: #1b1428; box-shadow: 0 5px 0 #120c1c; cursor: pointer; }
button.alt { background: #7fd3ff; }
button:hover { filter: brightness(1.08); }
button:active { transform: translateY(4px); box-shadow: 0 1px 0 #120c1c; }
button:disabled { opacity: 0.5; cursor: default; }
.note { font-size: 15px; opacity: 0.7; }
.status { margin-top: 20px; min-height: 28px; font-size: 20px; font-weight: 700; }
.status.err { color: #ff8a8a; }
"#;

const HUD_HTML: &str = r#"<div class="hud" id="hudRoot">
  <div class="room" id="room"></div>
  <div class="board" id="board"></div>
  <div class="feed" id="feed"></div>
  <div class="help">WASD / ARROWS move  ·  grab the coins  ·  ESC leave</div>
</div>"#;

const HUD_CSS: &str = r#"
.hud { position: absolute; inset: 0; font-family: system-ui, 'Segoe UI', sans-serif; color: #fff; text-shadow: 2px 2px 0 #120c1c; }
.hud.off { display: none; }
.room { position: absolute; left: 24px; top: 92px; font-size: 22px; font-weight: 800; background: rgb(18 12 28 / 0.6); padding: 10px 16px; border-radius: 12px; white-space: pre; line-height: 1.4; }
.board { position: absolute; right: 24px; top: 92px; min-width: 240px; background: rgb(18 12 28 / 0.6); padding: 12px 16px; border-radius: 12px; font-size: 22px; font-weight: 800; }
.board .row { display: flex; align-items: center; gap: 10px; padding: 3px 0; }
.board .dot { width: 18px; height: 18px; border-radius: 50%; border: 3px solid #120c1c; }
.board .n { flex: 1; }
.board .me { color: #ffd23a; }
.feed { position: absolute; left: 24px; bottom: 70px; font-size: 20px; font-weight: 700; display: flex; flex-direction: column; gap: 4px; }
.help { position: absolute; left: 50%; bottom: 22px; transform: translateX(-50%); font-size: 18px; font-weight: 700; opacity: 0.75; }
"#;

const SCRIPT: &str = r#"// LAN Party — multiplayer through `ctx.net` (see `demo_lan.rs`).
//
// Messages (all `ctx.net.send(data)` JSON):
//   { k: 's', x, y }            a player's position (20×/s, everyone)
//   { k: 'grab', i }            "I touched coin i" (to the host)
//   { k: 'world', coins, scores } the host's coins + scores (on change, 1×/s)
const ref = (id) => getCompByID(id)
const net = ctx.net
const keys = ctx.keysDown
const dt = Math.min(ctx.deltaSeconds || 1 / 60, 1 / 20)
const COLORS = [[0.5, 0.8, 1], [1, 0.55, 0.6], [0.6, 0.95, 0.55], [1, 0.8, 0.35], [0.8, 0.6, 1], [1, 0.65, 0.3], [0.45, 0.95, 0.9], [0.95, 0.95, 0.95]]
const SPEED = 260, SEND_EVERY = 0.05, R_PLAYER = 22, R_COIN = 13
const g = ctx.globals.lan || (ctx.globals.lan = {
  me: { x: 0, y: 0 }, others: {}, coins: [], scores: {}, sendT: 0, worldT: 0, feed: [], lobbyDom: null, hudDom: null, wired: false, wasOnline: false, pending: {},
})
const colorOf = (id) => COLORS[(id - 1) % COLORS.length]
const hex = (c) => '#' + c.map((v) => Math.round(v * 255).toString(16).padStart(2, '0')).join('')
const say = (text) => { g.feed.push({ text, t: 5 }); if (g.feed.length > 5) g.feed.shift() }
const randomCoin = () => [(Math.random() * 2 - 1) * (ARENA_X - 30), (Math.random() * 2 - 1) * (ARENA_Y - 30)]
const shareWorld = () => net.send({ k: 'world', coins: g.coins, scores: g.scores })

// ---------------------------------------------------------- lobby --
const lobby = ref('lobby').dom
if (lobby && g.lobbyDom !== lobby) {
  g.lobbyDom = lobby
  const $ = (id) => lobby.getElementById(id)
  try { const saved = ctx.storage.get('lanParty'); if (saved) { $('name').value = saved.name || 'Player'; $('addr').value = saved.addr || $('addr').value; $('port').value = saved.port || '7777' } } catch {}
  const remember = () => ctx.storage.set('lanParty', { name: $('name').value, addr: $('addr').value, port: $('port').value })
  $('host').addEventListener('click', () => { remember(); net.host({ port: Number($('port').value) || 7777, name: $('name').value || 'Host' }) })
  $('join').addEventListener('click', () => { remember(); net.join($('addr').value, { name: $('name').value || 'Player' }) })
  if (!net.canHost) {
    $('host').disabled = true
    $('hostNote').textContent = 'Hosting needs the desktop editor (or an1mbyt3-server). You can still join.'
  }
}
const online = net.status === 'online'
if (lobby) {
  const $ = (id) => lobby.getElementById(id)
  $('wrap').classList.toggle('off', online)
  const status = net.status === 'connecting' ? 'Connecting…' : net.status === 'error' ? net.error : ''
  if ($('status').textContent !== status) $('status').textContent = status
  $('status').classList.toggle('err', net.status === 'error')
}
if (online && keys.has('escape')) net.leave()

// ------------------------------------------------------- networking --
for (const e of net.events) {
  if (e.type === 'connected') {
    g.others = {}
    g.me = { x: (Math.random() * 2 - 1) * 200, y: (Math.random() * 2 - 1) * 100 }
    say(net.isHost ? 'You are hosting' : 'Joined the game')
    if (net.isHost) { g.coins = Array.from({ length: COINS }, randomCoin); g.scores = {} }
  } else if (e.type === 'join') {
    say(e.name + ' joined')
    if (net.isHost) shareWorld()
  } else if (e.type === 'leave') {
    say(e.name + ' left')
    delete g.others[e.id]
  } else if (e.type === 'host') {
    say((e.id === net.me ? 'You are' : net.nameOf(e.id) + ' is') + ' now the host')
  } else if (e.type === 'disconnected') {
    g.others = {}
  }
}
for (const m of net.messages) {
  const d = m.data
  if (!d || typeof d !== 'object') continue
  if (d.k === 's') {
    const o = g.others[m.from] || (g.others[m.from] = { x: d.x, y: d.y, tx: d.x, ty: d.y })
    o.tx = d.x; o.ty = d.y
  } else if (d.k === 'world' && !net.isHost) {
    g.coins = d.coins
    g.scores = d.scores
  } else if (d.k === 'grab' && net.isHost) {
    // The host decides: is that coin still there, and is that player
    // (as the host sees them) actually near it?
    const c = g.coins[d.i], o = g.others[m.from]
    if (c && o && Math.hypot(o.tx - c[0], o.ty - c[1]) < R_PLAYER + R_COIN + 40) {
      g.scores[m.from] = (g.scores[m.from] || 0) + 1
      g.coins[d.i] = randomCoin()
      shareWorld()
    }
  }
}

// ---------------------------------------------------------- you --
if (online) {
  let mx = 0, my = 0
  if (keys.has('a') || keys.has('left')) mx -= 1
  if (keys.has('d') || keys.has('right')) mx += 1
  if (keys.has('w') || keys.has('up')) my += 1
  if (keys.has('s') || keys.has('down')) my -= 1
  const l = Math.hypot(mx, my) || 1
  g.me.x = Math.max(-ARENA_X, Math.min(ARENA_X, g.me.x + (mx / l) * SPEED * dt))
  g.me.y = Math.max(-ARENA_Y, Math.min(ARENA_Y, g.me.y + (my / l) * SPEED * dt))
  g.sendT -= dt
  if (g.sendT <= 0) { g.sendT = SEND_EVERY; net.send({ k: 's', x: Math.round(g.me.x), y: Math.round(g.me.y) }) }
  // Coins you touch: the host takes them directly; others ask the host
  // (once per coin until the host's answer arrives).
  g.coins.forEach((c, i) => {
    if (Math.hypot(g.me.x - c[0], g.me.y - c[1]) > R_PLAYER + R_COIN) return
    if (net.isHost) {
      g.scores[net.me] = (g.scores[net.me] || 0) + 1
      g.coins[i] = randomCoin()
      shareWorld()
      ctx.playTone({ freq: 880, to: 1320, duration: 0.12, type: 'square', volume: 0.12 })
    } else if (!g.pending[i] || g.pending[i] < ctx.time - 0.5) {
      g.pending[i] = ctx.time || 1
      net.send({ k: 'grab', i }, net.hostId)
    }
  })
  if (net.isHost) {
    g.worldT -= dt
    if (g.worldT <= 0) { g.worldT = 1; shareWorld() }
  }
}
// Score went up: a ding for the grabber.
const myScore = g.scores[net.me] || 0
if (g.lastScore !== undefined && myScore > g.lastScore && !net.isHost) ctx.playTone({ freq: 880, to: 1320, duration: 0.12, type: 'square', volume: 0.12 })
g.lastScore = myScore

// ----------------------------------------------------------- draw --
// Players: you first (avatar 0), then everyone else by id.
const shown = online ? [{ id: net.me, x: g.me.x, y: g.me.y, you: true }] : []
for (const p of net.players) {
  if (p.id === net.me) continue
  const o = g.others[p.id]
  if (!o) continue
  // Smoothly chase the latest reported position.
  const k = Math.min(1, dt * 14)
  o.x += (o.tx - o.x) * k; o.y += (o.ty - o.y) * k
  shown.push({ id: p.id, x: o.x, y: o.y, you: false })
}
for (let i = 0; i < AVATARS; i++) {
  const av = ref('av' + i)
  const p = shown[i]
  if (!p) { if (av.visible) av.visible = false; continue }
  av.visible = true
  av.x = p.x; av.y = p.y
  // A little squash while moving.
  const c = colorOf(p.id)
  av.fillColor = [c[0], c[1], c[2], 1]
  const name = net.nameOf(p.id).toUpperCase()
  const tag = ref('av' + i + 'name')
  if (tag.textContent !== name) tag.textContent = name
  ref('av' + i + 'you').visible = p.you
}
for (let i = 0; i < COINS; i++) {
  const coin = ref('coin' + i)
  const c = online ? g.coins[i] : null
  coin.visible = !!c
  if (c) {
    coin.x = c[0]; coin.y = c[1] + Math.sin((ctx.time || 0) * 4 + i) * 3
    coin.scaleX = Math.abs(Math.cos((ctx.time || 0) * 3 + i)) * 0.7 + 0.3
  }
}

// ------------------------------------------------------------ HUD --
const hud = ref('hud').dom
if (hud) {
  const $ = (id) => hud.getElementById(id)
  $('hudRoot').classList.toggle('off', !online)
  if (online) {
    const room = net.isHost && net.hosting
      ? 'HOSTING on port ' + net.hosting.port + '\n' + net.hosting.addresses.map((a) => '  join: ' + a + ':' + net.hosting.port).join('\n')
      : 'CONNECTED  ·  host: ' + net.nameOf(net.hostId) + (net.isHost ? ' (you)' : '')
    if ($('room').textContent !== room) $('room').textContent = room
    const rows = net.players
      .map((p) => ({ p, s: g.scores[p.id] || 0 }))
      .sort((a, b) => b.s - a.s)
      .map(({ p, s }) => `<div class="row"><span class="dot" style="background:${hex(colorOf(p.id))}"></span><span class="n ${p.id === net.me ? 'me' : ''}">${p.name.replace(/[<&]/g, '')}${p.id === net.hostId ? ' ★' : ''}</span><span>${s}</span></div>`)
      .join('')
    if (g.boardHtml !== rows) { g.boardHtml = rows; $('board').innerHTML = rows }
  }
  g.feed.forEach((f) => { f.t -= dt })
  g.feed = g.feed.filter((f) => f.t > 0)
  const feed = g.feed.map((f) => `<div>${f.text.replace(/[<&]/g, '')}</div>`).join('')
  if (g.feedHtml !== feed) { g.feedHtml = feed; $('feed').innerHTML = feed }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_pools_the_script_drives() {
        let mut document = Document::new();
        let demo = build_demo_lan(&mut document);
        let artboard = document.artboard(demo.scene).unwrap();
        let ids: Vec<&str> = artboard.node_ids().filter_map(|id| artboard.node(id)).map(|n| n.id.as_str()).collect();
        for i in 0..AVATARS {
            assert!(ids.contains(&format!("av{i}name").as_str()));
        }
        for id in ["coin0", "lobby", "hud", "camera"] {
            assert!(ids.contains(&id), "missing {id}");
        }
    }
}
