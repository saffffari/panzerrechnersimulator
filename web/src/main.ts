import * as T from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { NativeTiger, SCALE, type DamageLevel } from "./native-tiger";
import {
  rounds,
  penetration,
  slope,
  parsePresets,
  shotOptions,
} from "./preset";
import { discHit, ringNormal, rotateOnRing, type RingAxis } from "./aim-rings";
import { RoomEnvironment } from "three/addons/environments/RoomEnvironment.js";
import { PlayView } from "./play-view";
import { Simulation } from "./simulation";
import "./style.css";
import { playbackStep, sectionSlide } from "./editor-time";
const $ = <E extends HTMLElement = HTMLElement>(id: string) =>
  document.getElementById(id) as E;
const select = (id: string) => $<HTMLSelectElement>(id);
const canvas = $<HTMLCanvasElement>("view");
const renderer = new T.WebGLRenderer({
  canvas,
  antialias: false,
  alpha: true,
  powerPreference: "low-power",
});
renderer.setPixelRatio(1);
renderer.shadowMap.enabled = true;
renderer.shadowMap.type = T.PCFSoftShadowMap;
renderer.shadowMap.autoUpdate = false;
renderer.localClippingEnabled = true;
renderer.toneMapping = T.ACESFilmicToneMapping;
renderer.toneMappingExposure = 1.0;
const scene = new T.Scene();
scene.background = new T.Color("#a7b4bc");
scene.fog = new T.Fog("#a7b4bc", 35, 90);
const pmrem = new T.PMREMGenerator(renderer);
const room = new RoomEnvironment();
scene.environment = pmrem.fromScene(room, 0.04).texture;
scene.environmentIntensity = 0.35;
room.dispose();
pmrem.dispose();
const camera = new T.PerspectiveCamera(46, 1, 0.025, 150);
camera.position.set(7, 5, 9);
const top = new T.OrthographicCamera(-2.8, 2.8, 4.9778, -4.9778, 0.1, 40);
top.position.set(0, 16, 1.1);
top.up.set(0, 0, 1);
top.lookAt(0, 0, 1.1);
top.updateMatrixWorld();
const planCamera = top.clone();
let active: T.Camera = camera;
const controls = new OrbitControls(camera, canvas);
controls.target.set(0, 1.5, 0.7);
controls.mouseButtons = {
  LEFT: null as any,
  MIDDLE: T.MOUSE.PAN,
  RIGHT: T.MOUSE.ROTATE,
};
controls.minDistance = 0.12;
controls.maxDistance = 35;
controls.enableDamping = false;
const ambient = new T.HemisphereLight("#dae4f1", "#44423b", 0.6);
scene.add(ambient);
const sun = new T.DirectionalLight("#fff1d7", 1.8);
sun.position.set(-5, 12, 8);
sun.castShadow = true;
sun.shadow.mapSize.set(1024, 1024);
Object.assign(sun.shadow.camera, {
  left: -7,
  right: 7,
  top: 9,
  bottom: -9,
  near: 0.1,
  far: 35,
});
sun.shadow.normalBias = 0.008;
sun.shadow.bias = -0.0001;
scene.add(sun);
const ground = new T.Mesh(
  new T.PlaneGeometry(200, 200),
  new T.MeshStandardMaterial({ color: "#9b9f9e", roughness: 1 }),
);
ground.rotation.x = -Math.PI / 2;
ground.position.y = -0.01;
ground.receiveShadow = true;
scene.add(ground);
const tiger = new NativeTiger();
scene.add(tiger.root);
let battleMode = false,
  play: PlayView | null = null;
const keys = new Set<string>();
function mode(battle: boolean) {
  if (!ready) return;
  battleMode = battle;
  if (battle && !play) {
    play = new PlayView(canvas);
    play.setPlayerModel(tiger.root.clone(true));
    play.scene.environment = scene.environment;
  }
  play?.setActive(battle);
  controls.enabled = !battle && active !== top;
  if (battle) {
    play!.reset(3);
    paused = false;
  } else paused = true;
  $("battle-mode").classList.toggle("active", battle);
  $("range-mode").classList.toggle("active", !battle);
  dirty = true;
  resize();
}
$("range-mode").onclick = () => mode(false);
$("battle-mode").onclick = () => mode(true);
addEventListener("keydown", (e) => {
  if (!(e.target as HTMLElement).matches("input,select")) keys.add(e.code);
});
addEventListener("keyup", (e) => keys.delete(e.code));
addEventListener("blur", () => keys.clear());
const sim = new Simulation(),
  fx = new T.InstancedMesh(
    new T.BoxGeometry(1, 1, 1),
    new T.MeshBasicMaterial(),
    1200,
  );
fx.count = 0;
fx.frustumCulled = false;
scene.add(fx);
const dummy = new T.Object3D();
const impact = new T.Vector3(-1.5, 1.5, 0.2),
  from = new T.Vector3(-1, 0, 0);
let hitNormal = new T.Vector3(-1, 0, 0),
  roundIndex = 4,
  range = 100,
  paused = true,
  ready = false,
  dirty = true,
  cutaway = false,
  lamps = true,
  damageTint = false,
  step = 1 / 60,
  timeScale = 1;
const helpers = new T.Group();
scene.add(helpers);
const sectionNormal = new T.Vector3(0, 1, 0),
  sectionPivot = new T.Vector3(0, 1.4, 0);
let sectionOn = false,
  sectionOffset = 0;
const sectionHandle = new T.Group();
const sectionOutline = new T.LineLoop(
  new T.BufferGeometry().setFromPoints([
    new T.Vector3(-3.5, -3.5, 0),
    new T.Vector3(3.5, -3.5, 0),
    new T.Vector3(3.5, 3.5, 0),
    new T.Vector3(-3.5, 3.5, 0),
  ]),
  new T.LineBasicMaterial({
    color: 0xffffff,
    transparent: true,
    opacity: 0.55,
    depthTest: false,
    depthWrite: false,
  }),
);
const sectionAxis = new T.Line(
  new T.BufferGeometry().setFromPoints([
    new T.Vector3(),
    new T.Vector3(0, 0, 0.8),
  ]),
  sectionOutline.material,
);
const sectionCenter = new T.Mesh(
  new T.PlaneGeometry(1, 1),
  new T.MeshBasicMaterial({
    color: 0xffffff,
    side: T.DoubleSide,
    depthTest: false,
    depthWrite: false,
  }),
);
sectionOutline.renderOrder = 1001;
sectionAxis.renderOrder = 1001;
sectionCenter.renderOrder = 1003;
sectionHandle.add(sectionOutline, sectionAxis, sectionCenter);
helpers.add(sectionHandle);
sectionHandle.visible = false;
function syncSection() {
  const center = sectionPivot
    .clone()
    .addScaledVector(sectionNormal, sectionOffset);
  tiger.section.normal.copy(sectionNormal).negate();
  tiger.section.constant = sectionOn ? sectionNormal.dot(center) : 100;
  sectionHandle.visible = sectionOn;
  sectionHandle.position.copy(center);
  sectionHandle.quaternion.setFromUnitVectors(
    new T.Vector3(0, 0, 1),
    sectionNormal,
  );
  dirty = true;
}
function screenPoint(point: T.Vector3) {
  const v = point.clone().project(active),
    r = canvas.getBoundingClientRect();
  return new T.Vector2(
    r.left + ((v.x + 1) * r.width) / 2,
    r.top + ((1 - v.y) * r.height) / 2,
  );
}

const guide = new T.Group();
helpers.add(guide);
guide.visible = false;
let ringsVisible = false;
const ringMeshes: Record<RingAxis, { line: T.LineLoop; fill: T.Mesh }> =
  {} as any;
for (const axis of ["azimuth", "elevation"] as const) {
  const pts = [];
  for (let i = 0; i < 128; i++) {
    const a = (i / 128) * Math.PI * 2;
    pts.push(new T.Vector3(Math.cos(a) * 0.85, Math.sin(a) * 0.85, 0));
  }
  const line = new T.LineLoop(
    new T.BufferGeometry().setFromPoints(pts),
    new T.LineBasicMaterial({
      color: axis === "azimuth" ? "#ed3333" : "#ffffff",
      depthTest: false,
    }),
  );
  const fill = new T.Mesh(
    new T.CircleGeometry(0.85, 96),
    new T.MeshBasicMaterial({
      color: axis === "azimuth" ? "#ed3333" : "#ffffff",
      transparent: true,
      opacity: 0.12,
      side: T.DoubleSide,
      depthTest: false,
      depthWrite: false,
    }),
  );
  fill.visible = false;
  line.renderOrder = 1002;
  fill.renderOrder = 1001;
  (line.material as T.Material).depthWrite = false;
  guide.add(line, fill);
  ringMeshes[axis] = { line, fill };
}
const marker = new T.LineSegments(
  new T.BufferGeometry().setFromPoints([
    new T.Vector3(-0.5, 0, 0),
    new T.Vector3(0.5, 0, 0),
    new T.Vector3(0, -0.5, 0),
    new T.Vector3(0, 0.5, 0),
  ]),
  new T.LineBasicMaterial({ color: "#ed3333", depthTest: false }),
);
marker.renderOrder = 1003;
marker.material.depthWrite = false;
helpers.add(marker);
const rayLine = new T.Line(
  new T.BufferGeometry().setFromPoints([new T.Vector3(), new T.Vector3()]),
  new T.LineBasicMaterial({
    color: "#ed3333",
    transparent: true,
    opacity: 0.6,
    depthTest: false,
  }),
);
rayLine.renderOrder = 1002;
rayLine.material.depthWrite = false;
helpers.add(rayLine);
function syncAim() {
  guide.position.copy(impact);
  marker.position.copy(impact);
  rayLine.geometry.setFromPoints([
    impact,
    impact.clone().addScaledVector(from, 1.3),
  ]);
  for (const axis of ["azimuth", "elevation"] as const) {
    const q = new T.Quaternion().setFromUnitVectors(
      new T.Vector3(0, 0, 1),
      ringNormal(axis, from),
    );
    ringMeshes[axis].line.quaternion.copy(q);
    ringMeshes[axis].fill.quaternion.copy(q);
  }
  dirty = true;
}
function resize() {
  const rect = canvas.getBoundingClientRect(),
    w = Math.min(+select("resolution").value, Math.round(rect.width)),
    h = Math.max(1, Math.round((w * rect.height) / rect.width));
  renderer.setSize(w, h, false);
  camera.aspect = rect.width / rect.height;
  camera.updateProjectionMatrix();
  top.left = -5 * camera.aspect;
  top.right = 5 * camera.aspect;
  top.top = 5;
  top.bottom = -5;
  top.updateProjectionMatrix();
  if (play) {
    play.camera.aspect = camera.aspect;
    play.camera.updateProjectionMatrix();
  }
  dirty = true;
}
addEventListener("resize", resize);
select("resolution").onchange = resize;
const shotList: { name: string; shot: any }[] = [];
function applyShot(s: any) {
  roundIndex = Math.max(0, Math.min(rounds.length - 1, s.shell ?? 4));
  range = s.range_m ?? 100;
  select("shell").value = String(roundIndex);
  const az = T.MathUtils.degToRad(s.from_az ?? 90),
    el = T.MathUtils.degToRad(s.from_el ?? 0);
  from.set(
    Math.sin(az) * Math.cos(el),
    Math.sin(el),
    Math.cos(az) * Math.cos(el),
  );
  if (s.target?.point) impact.fromArray(s.target.point).multiplyScalar(SCALE);
  syncAim();
}
for (const [i, r] of rounds.entries())
  select("shell").add(new Option(r.name, String(i)));
select("shell").onchange = () => (roundIndex = +select("shell").value);
select("step").onchange = () => (step = +select("step").value);
select("shot").onchange = () =>
  applyShot(shotList[+select("shot").value]?.shot ?? {});
let recordings: MediaRecorder | null = null;
let chunks: Blob[] = [];
$("record").onclick = () => {
  if (recordings) {
    recordings.stop();
    return;
  }
  const stream = canvas.captureStream(30);
  recordings = new MediaRecorder(stream);
  chunks = [];
  recordings.ondataavailable = (e) => chunks.push(e.data);
  recordings.onstop = () => {
    download(
      URL.createObjectURL(new Blob(chunks, { type: recordings!.mimeType })),
      "panzer-range.webm",
    );
    stream.getTracks().forEach((t) => t.stop());
    recordings = null;
    $("record").textContent = "● REC";
  };
  recordings.start();
  $("record").textContent = "■ STOP";
};
function download(url: string, name: string) {
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
}
function say(s: string) {
  $("status").textContent = s;
}
function reset() {
  if (battleMode) {
    play!.reset(3);
    paused = true;
    dirty = true;
    return;
  }
  sim.reset();
  fx.count = 0;
  paused = true;
  time = 0;
  tiger.reset();
  for (const c of [...scars.children]) {
    scars.remove(c);
    (c as T.Mesh).geometry.dispose();
  }
  for (const [m, g] of altered) {
    m.geometry.dispose();
    m.geometry = g;
  }
  altered.clear();
  tiger.setRoof(cutaway);
  tiger.tintDamage(damageTint);
  renderer.shadowMap.needsUpdate = true;
  updateHud();
  dirty = true;
  say("RESET · FRESH TIGER · PAUSED");
  $("pause").textContent = "▶ RESUME";
}
$("reset").onclick = reset;
$("pause").onclick = () => {
  paused = !paused;
  $("pause").textContent = paused ? "▶ RESUME" : "Ⅱ PAUSE";
  dirty = true;
};
const scars = new T.Group();
scene.add(scars);
const altered = new Map<T.Mesh, T.BufferGeometry>();
function fire() {
  if (!ready) return;
  if (battleMode) {
    play!.fire(rounds[roundIndex]);
    dirty = true;
    return;
  }
  const ray = new T.Raycaster(
    impact.clone().addScaledVector(from, 12),
    from.clone().negate(),
    0,
    24,
  );
  const hits = ray
    .intersectObjects(tiger.meshes, false)
    .filter(
      (h) =>
        !h.object.userData.module?.match(
          /Commander|Driver|Gunner|Loader|Radio/,
        ),
    );
  const hit = hits.find((h) => h.point.distanceTo(impact) < 3) ?? hits[0];
  if (!hit) {
    say("NO ARMOR INTERSECTION · PLACE SHOT ON TIGER");
    return;
  }
  const mesh = hit.object as T.Mesh;
  impact.copy(hit.point);
  hitNormal = hit.face!.normal.clone().transformDirection(mesh.matrixWorld);
  const r = rounds[roundIndex],
    angle =
      (Math.acos(Math.min(1, Math.abs(from.dot(hitNormal)))) * 180) / Math.PI,
    plate =
      mesh.userData.module === "MainGun"
        ? 100
        : Math.abs(hitNormal.y) > 0.6
          ? 25
          : Math.abs(hitNormal.z) > 0.6
            ? 100
            : 80;
  const effective = plate * slope(r, angle, plate),
    passed = penetration(r, range) > effective && angle < 85;
  const module = mesh.userData.module ?? "Hull";
  tiger.setStatus(
    module,
    passed ? (tiger.statuses.get(module) === 1 ? 2 : 1) : 1,
  );
  if (passed) {
    const radius = Math.min(0.45, Math.max(0.055, r.caliber_mm / 1500));
    if (!altered.has(mesh)) altered.set(mesh, mesh.geometry);
    const geo = mesh.geometry.clone(),
      pos = geo.getAttribute("position"),
      idx = geo.getIndex(),
      kept: number[] = [];
    const p = new T.Vector3();
    for (let i = 0; i < (idx?.count ?? pos.count); i += 3) {
      const ids = [0, 1, 2].map((k) => (idx ? idx.getX(i + k) : i + k));
      p.set(0, 0, 0);
      for (const id of ids) p.add(new T.Vector3().fromBufferAttribute(pos, id));
      p.multiplyScalar(1 / 3).applyMatrix4(mesh.matrixWorld);
      if (p.distanceTo(impact) > radius) kept.push(...ids);
    }
    geo.setIndex(kept);
    mesh.geometry = geo;
    const trajectory = new T.Ray(
      impact.clone().addScaledVector(from, -0.03),
      from.clone().negate(),
    );
    for (const crew of tiger.crew) {
      const box = new T.Box3().setFromObject(crew).expandByScalar(0.08);
      if (trajectory.intersectsBox(box)) {
        tiger.setStatus(crew.name, 2);
        for (let k = 0; k < 100; k++)
          sim.emit(
            box.getCenter(new T.Vector3()),
            new T.Vector3(
              (Math.random() - 0.5) * 3,
              Math.random() * 2,
              (Math.random() - 0.5) * 3,
            ),
            2,
            0.00375,
            2,
          );
      }
    }
  }
  const mark = new T.Mesh(
    new T.RingGeometry(0.025, passed ? 0.065 : 0.045, 16),
    new T.MeshBasicMaterial({ color: "#301c11", side: T.DoubleSide }),
  );
  mark.position.copy(impact).addScaledVector(hitNormal, 0.008);
  mark.quaternion.setFromUnitVectors(new T.Vector3(0, 0, 1), hitNormal);
  scars.add(mark);
  if (scars.children.length > 32) {
    const old = scars.children[0] as T.Mesh;
    scars.remove(old);
    old.geometry.dispose();
  }
  for (let i = 0; i < 100; i++)
    sim.emit(
      impact,
      new T.Vector3(
        (Math.random() - 0.5) * 6,
        Math.random() * 5,
        (Math.random() - 0.5) * 6,
      ),
      1,
      0.02,
      0.3 + Math.random() * 0.7,
    );
  paused = false;
  sim.shots++;
  syncAim();
  tiger.tintDamage(damageTint);
  renderer.shadowMap.needsUpdate = true;
  updateHud();
  say(
    `${r.name.toUpperCase()} · ${passed ? "PENETRATION" : "STOPPED"} · ${Math.round(effective)} MM EFFECTIVE`,
  );
  $("pause").textContent = "Ⅱ PAUSE";
}
$("fire").onclick = fire;
function setCamera(name: string) {
  active = name === "top" ? top : camera;
  controls.enabled = name !== "top";
  camera.up.set(0, 1, 0);
  if (name === "free") {
    camera.position.set(7, 5, 9);
    controls.target.set(0, 1.5, 0.7);
  } else if (name !== "top") {
    const at =
      name === "driver"
        ? new T.Vector3(0.535, 1.65, 1.78)
        : name === "gunner"
          ? new T.Vector3(0.5, 2.15, 0.27)
          : new T.Vector3(0.564, 2.96, -0.4);
    camera.position.copy(at).multiplyScalar(SCALE);
    controls.target.copy(camera.position).add(new T.Vector3(0, -0.06, 1));
  }
  controls.update();
  dirty = true;
}
select("camera").onchange = () => setCamera(select("camera").value);
let down: { x: number; y: number; t: number } | null = null,
  drag: { axis: RingAxis; normal: T.Vector3; initial: T.Vector3 } | null = null;
let sectionDrag: {
  offset: number;
  x: number;
  y: number;
  axis: T.Vector2;
} | null = null;
const picker = new T.Raycaster();
function pointer(e: PointerEvent) {
  const r = canvas.getBoundingClientRect();
  picker.setFromCamera(
    new T.Vector2(
      ((e.clientX - r.left) / r.width) * 2 - 1,
      1 - ((e.clientY - r.top) / r.height) * 2,
    ),
    active,
  );
}
function finishRing() {
  for (const axis of ["azimuth", "elevation"] as const) {
    ringMeshes[axis].fill.visible = false;
    (ringMeshes[axis].line.material as T.LineBasicMaterial).color.set(
      axis === "azimuth" ? "#ed3333" : "#ffffff",
    );
  }
  drag = null;
  dirty = true;
}
canvas.oncontextmenu = (e) => e.preventDefault();
canvas.onpointerdown = (e) => {
  if (e.button !== 0) return;
  if (battleMode) {
    play!.pick(e.clientX, e.clientY, canvas);
    dirty = true;
    return;
  }
  if (sectionOn) {
    const center = screenPoint(sectionHandle.position),
      rect = canvas.getBoundingClientRect();
    if (center.distanceTo(new T.Vector2(e.clientX, e.clientY)) < 14) {
      const axis = screenPoint(
        sectionHandle.position.clone().add(sectionNormal),
      ).sub(center);
      axis.set(axis.x / rect.width, axis.y / rect.height);
      sectionDrag = { offset: sectionOffset, x: e.clientX, y: e.clientY, axis };
      canvas.setPointerCapture(e.pointerId);
      return;
    }
  }
  pointer(e);
  down = { x: e.clientX, y: e.clientY, t: performance.now() };
  const screen = impact.clone().project(active),
    rect = canvas.getBoundingClientRect(),
    mx = rect.left + ((screen.x + 1) * rect.width) / 2,
    my = rect.top + ((1 - screen.y) * rect.height) / 2;
  if (Math.hypot(e.clientX - mx, e.clientY - my) < 12) {
    ringsVisible = true;
    guide.visible = true;
    dirty = true;
    down = null;
    return;
  }
  if (ringsVisible) {
    const candidates = (["azimuth", "elevation"] as const)
      .map((axis) => {
        const normal = ringNormal(axis, from),
          hit = discHit(picker.ray, impact, normal, 0.85);
        return { axis, normal, hit };
      })
      .filter((x) => x.hit);
    candidates.sort(
      (a, b) =>
        Math.abs(a.hit!.distanceTo(impact) - 0.85) -
        Math.abs(b.hit!.distanceTo(impact) - 0.85),
    );
    const chosen = candidates[0];
    if (chosen) {
      drag = {
        axis: chosen.axis,
        normal: chosen.normal,
        initial: from.clone(),
      };
      ringMeshes[drag.axis].fill.visible = true;
      dirty = true;
    }
  }
  canvas.setPointerCapture(e.pointerId);
};
canvas.onpointermove = (e) => {
  if (sectionDrag) {
    const r = canvas.getBoundingClientRect(),
      d = sectionDrag;
    sectionOffset = T.MathUtils.clamp(
      d.offset +
        sectionSlide(
          (e.clientX - d.x) / r.width,
          (e.clientY - d.y) / r.height,
          d.axis.x,
          d.axis.y,
        ),
      -6,
      6,
    );
    syncSection();
    return;
  }
  if (!down || !drag) return;
  pointer(e);
  const p = picker.ray.intersectPlane(
    new T.Plane().setFromNormalAndCoplanarPoint(drag.normal, impact),
    new T.Vector3(),
  );
  if (p) {
    from.copy(rotateOnRing(drag.axis, p, impact, drag.initial));
    syncAim();
  }
};
canvas.onpointerup = (e) => {
  if (sectionDrag) {
    sectionDrag = null;
    return;
  }
  if (!down) return;
  if (!drag && Math.hypot(e.clientX - down.x, e.clientY - down.y) < 6) {
    pointer(e);
    const hit = picker
      .intersectObjects(tiger.meshes, false)
      .find((h) => h.object.visible);
    if (hit) {
      impact.copy(hit.point);
      hitNormal = hit
        .face!.normal.clone()
        .transformDirection(hit.object.matrixWorld);
      syncAim();
    }
    ringsVisible = false;
    guide.visible = false;
  }
  down = null;
  finishRing();
};
canvas.onpointercancel = () => {
  sectionDrag = null;
  down = null;
  finishRing();
};
controls.addEventListener("change", () => (dirty = true));
addEventListener("keydown", (e) => {
  if ((e.target as HTMLElement).matches("input,select,button")) return;
  if (e.code === "KeyF") {
    e.preventDefault();
    fire();
  }
  if (e.code === "Space") {
    e.preventDefault();
    $("pause").click();
  }
  if (e.code === "Backspace") {
    e.preventDefault();
    reset();
  }
});
function png(w: number, h: number, crewIndex?: number) {
  const target = new T.WebGLRenderTarget(w, h, {
    format: T.RGBAFormat,
    type: T.UnsignedByteType,
    depthBuffer: true,
  });
  const bg = scene.background,
    fog = scene.fog,
    roof = tiger.roof,
    sectionConstant = tiger.section.constant,
    visible = [ground.visible, helpers.visible, fx.visible, scars.visible];
  const origs = tiger.root.children.map((x) => [x, x.visible] as const);
  scene.background = null;
  scene.fog = null;
  ground.visible = helpers.visible = fx.visible = scars.visible = false;
  tiger.section.constant = 100;
  tiger.setRoof(true);
  tiger.tintDamage(true);
  let captureScene = scene,
    captureCamera: T.Camera = planCamera;
  if (crewIndex !== undefined) {
    captureScene = new T.Scene();
    captureScene.environment = scene.environment;
    captureScene.environmentIntensity = 0.9;
    captureScene.add(
      new T.HemisphereLight("#fff3e1", "#585d67", 2),
      new T.AmbientLight("white", 0.6),
    );
    const group = tiger.crew[crewIndex].clone(true);
    group.position.set(0, 0, 0);
    group.rotation.set(0, 0, 0);
    group.scale.setScalar(1);
    captureScene.add(group);
    const sun2 = new T.DirectionalLight("white", 2);
    sun2.position.set(2, 4, 4);
    captureScene.add(sun2);
    const b = new T.Box3().setFromObject(group),
      c = b.getCenter(new T.Vector3()),
      size = b.getSize(new T.Vector3());
    const height = size.y * 1.15,
      cam = new T.OrthographicCamera(
        (-height * w) / h / 2,
        (height * w) / h / 2,
        height / 2,
        -height / 2,
        0.1,
        10,
      );
    cam.position.set(c.x, c.y, c.z + 4);
    cam.lookAt(c);
    captureCamera = cam;
  }
  renderer.setRenderTarget(target);
  renderer.setClearColor(0, 0);
  renderer.shadowMap.needsUpdate = true;
  renderer.render(captureScene, captureCamera);
  const bytes = new Uint8Array(w * h * 4);
  renderer.readRenderTargetPixels(target, 0, 0, w, h, bytes);
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const ctx = c.getContext("2d")!,
    img = ctx.createImageData(w, h);
  for (let y = 0; y < h; y++)
    img.data.set(
      bytes.subarray((h - 1 - y) * w * 4, (h - y) * w * 4),
      y * w * 4,
    );
  ctx.putImageData(img, 0, 0);
  renderer.setRenderTarget(null);
  target.dispose();
  scene.background = bg;
  scene.fog = fog;
  [ground.visible, helpers.visible, fx.visible, scars.visible] = visible;
  for (const [o, v] of origs) o.visible = v;
  tiger.section.constant = sectionConstant;
  tiger.setRoof(roof);
  tiger.tintDamage(damageTint);
  renderer.shadowMap.needsUpdate = true;
  dirty = true;
  return c.toDataURL("image/png");
}
function updateHud() {
  if (!ready) return;
  $<HTMLImageElement>("plan").src = png(135, 240);
  $("crew").replaceChildren(
    ...tiger.crew.map((c, i) => {
      const img = new Image();
      img.src = png(28, 60, i);
      img.alt = `${c.name}: ${["intact", "damaged", "destroyed"][tiger.statuses.get(c.name) || 0]}`;
      return img;
    }),
  );
}
const widths = [
  540, 405, 270, 180, 135, 96, 72, 54, 36, 24, 16, 12, 8, 4, 2, 1,
];
async function resolutionStudy() {
  const files: { name: string; data: string }[] = [];
  for (const w of widths) {
    const h = Math.max(1, Math.round((w * 16) / 9));
    files.push({ name: `tiger-cutaway-${w}x${h}.png`, data: png(w, h) });
    await new Promise((r) => setTimeout(r, 0));
  }
  return files;
}
function settings() {
  return {
    name: "Web editor preset",
    settings: {
      shot: {
        shell: roundIndex,
        range_m: range,
        from_az: T.MathUtils.radToDeg(Math.atan2(from.x, from.z)),
        from_el: T.MathUtils.radToDeg(Math.asin(from.y)),
        target: { point: impact.clone().divideScalar(SCALE).toArray() },
      },
      camera: {
        eye: camera.position.toArray(),
        target: controls.target.toArray(),
      },
      scene: { cutaway, lamps },
      resolution: +select("resolution").value,
    },
  };
}
$("save").onclick = () =>
  download(
    URL.createObjectURL(
      new Blob([JSON.stringify(settings(), null, 2)], {
        type: "application/json",
      }),
    ),
    "panzer-editor-preset.json",
  );
$("load").onclick = () => $("file").click();
$<HTMLInputElement>("file").onchange = async (e) => {
  const file = (e.target as HTMLInputElement).files?.[0];
  if (!file) return;
  try {
    const p = parsePresets(JSON.parse(await file.text()))[0];
    applyShot(p.settings.shot ?? {});
    if (p.settings.camera?.eye) {
      camera.position.fromArray(p.settings.camera.eye);
      controls.target.fromArray(p.settings.camera.target ?? [0, 1.5, 0]);
      controls.update();
    }
    say("PRESET LOADED");
  } catch {
    say("INVALID PRESET JSON");
  }
};
const inspector = $("inspector");
let openPanel = "";
function rangeControl(
  label: string,
  min: number,
  max: number,
  value: number,
  fn: (v: number) => void,
  increment = 0.01,
) {
  const row = document.createElement("label");
  row.textContent = label;
  const input = document.createElement("input");
  input.type = "range";
  Object.assign(input, {
    min: String(min),
    max: String(max),
    step: String(increment),
    value: String(value),
  });
  const out = document.createElement("output");
  out.textContent = String(value);
  input.oninput = () => {
    out.textContent = input.value;
    fn(+input.value);
    dirty = true;
  };
  row.append(input, out);
  inspector.append(row);
}
function toggle(label: string, value: boolean, fn: (v: boolean) => void) {
  const row = document.createElement("label");
  row.textContent = label;
  const input = document.createElement("input");
  input.type = "checkbox";
  input.checked = value;
  input.onchange = () => {
    fn(input.checked);
    renderer.shadowMap.needsUpdate = true;
    dirty = true;
  };
  row.append(input);
  inspector.append(row);
}
function button(label: string, fn: () => void) {
  const b = document.createElement("button");
  b.textContent = label;
  b.onclick = fn;
  inspector.append(b);
}
function panel(name: string) {
  if (openPanel === name) {
    inspector.hidden = true;
    openPanel = "";
    return;
  }
  openPanel = name;
  inspector.hidden = false;
  inspector.replaceChildren();
  const h = document.createElement("h2");
  h.textContent = name.toUpperCase();
  inspector.append(h);
  switch (name) {
    case "shot":
      rangeControl("RANGE / M", 0, 3000, range, (v) => (range = v), 10);
      rangeControl(
        "BEARING / °",
        -180,
        180,
        T.MathUtils.radToDeg(Math.atan2(from.x, from.z)),
        (v) => {
          const el = Math.asin(from.y);
          from.set(
            Math.sin((v * Math.PI) / 180) * Math.cos(el),
            Math.sin(el),
            Math.cos((v * Math.PI) / 180) * Math.cos(el),
          );
          syncAim();
        },
        1,
      );
      rangeControl(
        "ELEVATION / °",
        -85,
        85,
        0,
        (v) => {
          const az = Math.atan2(from.x, from.z),
            el = (v * Math.PI) / 180;
          from.set(
            Math.sin(az) * Math.cos(el),
            Math.sin(el),
            Math.cos(az) * Math.cos(el),
          );
          syncAim();
        },
        1,
      );
      button("ADD SHOT", () => {
        shotList.push({
          name: `SHOT ${shotList.length + 1}`,
          shot: settings().settings.shot,
        });
        select("shot").add(
          new Option(shotList.at(-1)!.name, String(shotList.length - 1)),
        );
        select("shot").value = String(shotList.length - 1);
      });
      break;
    case "section":
      toggle("REMOVE ROOFS / DECKS", cutaway, (v) => {
        cutaway = v;
        tiger.setRoof(v);
      });
      toggle("SECTION PLANE", sectionOn, (v) => {
        sectionOn = v;
        syncSection();
      });
      for (const [name, n] of [
        ["X", [1, 0, 0]],
        ["Y", [0, 1, 0]],
        ["Z", [0, 0, 1]],
      ] as const) {
        button(`NORMAL ${name}`, () => {
          sectionNormal.fromArray(n);
          sectionOn = true;
          syncSection();
        });
      }
      button("FLIP NORMAL", () => {
        sectionNormal.negate();
        sectionOffset = -sectionOffset;
        syncSection();
      });
      rangeControl("NORMAL OFFSET / M", -6, 6, sectionOffset, (v) => {
        sectionOffset = v;
        sectionOn = true;
        syncSection();
      });
      button("CLEAR SECTION", () => {
        sectionOn = false;
        syncSection();
      });
      break;
    case "scene":
      toggle("CABIN LAMPS", lamps, (v) => {
        lamps = v;
        tiger.setLamps(v);
      });
      rangeControl("LAMP POWER", 0.1, 4, 1, (v) => tiger.setLamps(lamps, v));
      toggle("GROUND", ground.visible, (v) => (ground.visible = v));
      button("RESET TANK", reset);
      break;
    case "materials":
      toggle("DAMAGE COLORS", damageTint, (v) => {
        damageTint = v;
        tiger.tintDamage(v);
      });
      break;
    case "sky":
      rangeControl("SUN HEIGHT", 0, 20, sun.position.y, (v) => {
        sun.position.y = v;
        renderer.shadowMap.needsUpdate = true;
      });
      rangeControl("DAYLIGHT", 0, 5, sun.intensity, (v) => (sun.intensity = v));
      rangeControl(
        "SKY FILL",
        0,
        2,
        ambient.intensity,
        (v) => (ambient.intensity = v),
      );
      break;
    case "camera":
      rangeControl(
        "FIELD OF VIEW",
        25,
        90,
        camera.fov,
        (v) => {
          camera.fov = v;
          camera.updateProjectionMatrix();
        },
        1,
      );
      button("EXTERIOR", () => {
        select("camera").value = "free";
        setCamera("free");
      });
      button("TOP DOWN", () => {
        select("camera").value = "top";
        setCamera("top");
      });
      break;
    case "look":
      rangeControl(
        "EXPOSURE",
        0.2,
        3,
        renderer.toneMappingExposure,
        (v) => (renderer.toneMappingExposure = v),
      );
      button("PNG / CUTAWAY", () =>
        download(png(540, 960), "tiger-cutaway-540x960.png"),
      );
      button("RESOLUTION STUDY", async () => {
        say("RENDERING RESOLUTION STUDY…");
        const files = await resolutionStudy();
        $("gallery").replaceChildren(
          ...files.map((f) => {
            const fig = document.createElement("figure"),
              img = new Image(),
              cap = document.createElement("figcaption"),
              a = document.createElement("a");
            img.src = f.data;
            cap.textContent = f.name;
            a.href = f.data;
            a.download = f.name;
            a.append(img);
            fig.append(a, cap);
            return fig;
          }),
        );
        $<HTMLDialogElement>("resolution-dialog").showModal();
        say("RESOLUTION STUDY READY · CLICK IMAGE TO SAVE");
      });
      break;
    case "effects":
      toggle("SHOT GUIDE", helpers.visible, (v) => (helpers.visible = v));
      toggle("PARTICLES", fx.visible, (v) => (fx.visible = v));
      rangeControl("TIME SPEED", 0.1, 2, timeScale, (v) => (timeScale = v));
      break;
    case "tissue": {
      const p = document.createElement("p");
      p.textContent =
        "Crew status follows intersections with the authored crew figures. Orange means damaged; red means destroyed. The browser uses a bounded damage approximation; the native tissue solver remains the reference.";
      inspector.append(p);
      break;
    }
  }
}
document
  .querySelectorAll<HTMLButtonElement>("[data-panel]")
  .forEach((b) => (b.onclick = () => panel(b.dataset.panel!)));
$("close-gallery").onclick = () =>
  $<HTMLDialogElement>("resolution-dialog").close();
let last = performance.now(),
  time = 0,
  acc = 0,
  frames = 0,
  elapsed = 0;
function tick(now: number) {
  requestAnimationFrame(tick);
  const dt = Math.min(0.1, (now - last) / 1000);
  last = now;
  if (!ready || document.hidden) return;
  if (battleMode) {
    if (!paused) {
      play!.battle.step(dt * timeScale, {
        forward:
          Number(keys.has("KeyW") || keys.has("ArrowUp")) -
          Number(keys.has("KeyS") || keys.has("ArrowDown")),
        turn:
          Number(keys.has("KeyA") || keys.has("ArrowLeft")) -
          Number(keys.has("KeyD") || keys.has("ArrowRight")),
      });
    }
    play!.sync();
    renderer.render(play!.scene, play!.camera);
    say(play!.hud());
    return;
  }
  elapsed += dt;
  frames++;
  if (!paused) {
    acc += dt * timeScale;
    const sample = playbackStep(step, timeScale);
    for (let i = 0; i < 8 && acc + sample * 1e-5 >= sample; i++) {
      sim.step(sample);
      time += sample;
      acc = Math.max(0, acc - sample);
    }
    if (acc > sample * 8) acc = 0;
    dirty ||= sim.particles.length > 0 || fx.count > 0;
  }
  if (dirty || recordings) {
    let count = 0;
    for (const p of sim.particles) {
      dummy.position.copy(p.p);
      dummy.scale.setScalar(p.size);
      dummy.updateMatrix();
      fx.setMatrixAt(count, dummy.matrix);
      fx.setColorAt(
        count,
        new T.Color(
          p.kind === 2 ? "#641d15" : p.kind === 3 ? "#777777" : "#ffac55",
        ),
      );
      count++;
    }
    fx.count = count;
    fx.instanceMatrix.needsUpdate = true;
    if (fx.instanceColor) fx.instanceColor.needsUpdate = true;
    const d = impact
      .clone()
      .sub(camera.position)
      .dot(camera.getWorldDirection(new T.Vector3()));
    const pixel =
      active === top
        ? (top.top - top.bottom) / canvas.height
        : (2 *
            Math.max(0.01, d) *
            Math.tan(T.MathUtils.degToRad(camera.fov / 2))) /
          canvas.height;
    marker.scale.setScalar(pixel * 7);
    sectionCenter.scale.setScalar(pixel * 5);
    sectionCenter.quaternion
      .copy(sectionHandle.quaternion)
      .invert()
      .multiply(active.quaternion);
    marker.quaternion.copy(active.quaternion);
    renderer.render(scene, active);
    dirty = false;
  }
  if (elapsed > 1) {
    $("metrics").textContent =
      `${renderer.domElement.width} × ${renderer.domElement.height} · ${renderer.info.render.calls} DRAWS · ${time.toFixed(2)} S`;
    elapsed = frames = 0;
  }
}
async function boot() {
  try {
    await tiger.load();
    ready = true;
    renderer.shadowMap.needsUpdate = true;
    resize();
    controls.update();
    syncAim();
    const response = await fetch("/native-preset.json");
    const p = shotOptions(parsePresets(await response.json()));
    for (const x of p.choices)
      shotList.push({ name: x.name, shot: x.settings.shot ?? {} });
    if (!shotList.length)
      shotList.push({
        name: "SHOT 1",
        shot: {
          shell: 4,
          range_m: 100,
          from_az: -90,
          target: { point: [-1.7, 1.6, 0.2] },
        },
      });
    shotList.forEach((s, i) =>
      select("shot").add(new Option(s.name, String(i))),
    );
    select("shot").value = String(p.selected);
    applyShot(shotList[p.selected]?.shot ?? shotList[0].shot);
    updateHud();
    $("loading").hidden = true;
    say("READY · F FIRE · SPACE PAUSE · BACKSPACE RESET");
    requestAnimationFrame(tick);
  } catch (e) {
    $("loading").textContent = `Unable to load Tiger: ${e}`;
    throw e;
  }
}
(window as any).panzerEditor = {
  snapshot: () => ({
    ready,
    time,
    timeScale,
    particle: sim.particles[0]?.p.toArray() ?? null,
    section: {
      on: sectionOn,
      offset: sectionOffset,
      normal: sectionNormal.toArray(),
      center: sectionHandle.position.toArray(),
    },
    battleMode,
    ringsVisible,
    activeRing: drag?.axis ?? null,
    from: from.toArray(),
    meshes: tiger.meshes.length,
    statuses: Object.fromEntries(tiger.statuses),
    roundIndex,
    impact: impact.toArray(),
    cutaway,
    paused,
    draws: renderer.info.render.calls,
    triangles: renderer.info.render.triangles,
    pixels: [canvas.width, canvas.height],
  }),
  screenPoint: (p?: number[]) => {
    const v = (p ? new T.Vector3().fromArray(p) : impact.clone()).project(
        active,
      ),
      r = canvas.getBoundingClientRect();
    return [
      r.left + ((v.x + 1) * r.width) / 2,
      r.top + ((1 - v.y) * r.height) / 2,
    ];
  },
  setTimeScale: (v: number) => {
    timeScale = v;
  },
  setSection: (normal: number[], offset: number = 0) => {
    sectionNormal.fromArray(normal).normalize();
    sectionOffset = offset;
    sectionOn = true;
    syncSection();
  },
  hudAtlas: () => {
    const kept = new Map(tiger.statuses),
      result: { name: string; data: string }[] = [];
    tiger.statuses.clear();
    result.push({ name: "tank", data: png(135, 240) });
    const names = [
      ...new Set(tiger.meshes.map((m) => m.userData.module as string)),
    ].filter(Boolean);
    for (const name of names) {
      for (const state of [1, 2] as const) {
        tiger.statuses.set(name, state);
        result.push({ name: `tank-${name}-${state}`, data: png(135, 240) });
      }
      tiger.statuses.clear();
    }
    tiger.crew.forEach((crew, i) => {
      for (const state of [0, 1, 2] as const) {
        tiger.statuses.set(crew.name, state);
        result.push({
          name: `crew-${crew.name}-${state}`,
          data: png(28, 60, i),
        });
      }
      tiger.statuses.clear();
    });
    tiger.statuses = kept;
    tiger.tintDamage(damageTint);
    return result;
  },
  capture: png,
  resolutionStudy,
  fire,
  reset,
  setCamera,
  setRoof: (v: boolean) => {
    cutaway = v;
    tiger.setRoof(v);
    renderer.shadowMap.needsUpdate = true;
    dirty = true;
  },
  setStatus: (k: string, v: DamageLevel) => {
    tiger.setStatus(k, v);
    updateHud();
  },
  aim: (p: number[], d: number[]) => {
    impact.fromArray(p);
    from.fromArray(d).normalize();
    syncAim();
  },
};
boot();
