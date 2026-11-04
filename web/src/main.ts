import * as T from "three";
import { mergeGeometries } from "three/addons/utils/BufferGeometryUtils.js";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import "./style.css";
import { Simulation } from "./simulation";
import {
  rounds,
  parsePresets,
  shotOptions,
  finite,
  type Preset,
} from "./preset";
const canvas = document.querySelector<HTMLCanvasElement>("#view")!;
const status = document.querySelector<HTMLElement>("#status")!;
const shotSelect = document.querySelector<HTMLSelectElement>("#shot")!;
const stepSelect = document.querySelector<HTMLSelectElement>("#step")!;
const renderer = new T.WebGLRenderer({
  canvas,
  antialias: true,
  powerPreference: "low-power",
});
renderer.setPixelRatio(Math.min(devicePixelRatio, 1.5));
renderer.setSize(innerWidth, innerHeight);
renderer.shadowMap.enabled = true;
renderer.shadowMap.type = T.PCFSoftShadowMap;
renderer.toneMapping = T.ACESFilmicToneMapping;
renderer.toneMappingExposure = 1.15;
const scene = new T.Scene();
scene.background = new T.Color("#889ba3");
scene.fog = new T.Fog("#889ba3", 24, 70);
const camera = new T.PerspectiveCamera(43, innerWidth / innerHeight, 0.05, 100);
camera.position.set(-9, 6, 10);
const controls = new OrbitControls(camera, canvas);
controls.target.set(0, 1.5, 0);
controls.mouseButtons = {
  LEFT: null as any,
  MIDDLE: T.MOUSE.PAN,
  RIGHT: T.MOUSE.ROTATE,
};
controls.enableDamping = true;
controls.minDistance = 1;
controls.maxDistance = 30;
controls.maxPolarAngle = Math.PI * 0.49;
scene.add(new T.HemisphereLight("#d1e8f5", "#444739", 2));
const sun = new T.DirectionalLight("#ffe7ba", 3);
sun.position.set(-8, 13, 5);
sun.castShadow = true;
sun.shadow.mapSize.set(1024, 1024);
Object.assign(sun.shadow.camera, {
  left: -9,
  right: 9,
  top: 9,
  bottom: -9,
  near: 1,
  far: 35,
});
sun.shadow.bias = -0.0005;
sun.shadow.normalBias = 0.025;
scene.add(sun);
const ground = new T.Mesh(
  new T.PlaneGeometry(160, 160),
  new T.MeshStandardMaterial({ color: "#585c4a", roughness: 1 }),
);
ground.rotation.x = -Math.PI / 2;
ground.receiveShadow = true;
scene.add(ground);
const grid = new T.GridHelper(40, 40, "#7d8069", "#676d5c");
grid.position.y = 0.006;
scene.add(grid);
const cube = new T.BoxGeometry(1, 1, 1),
  dummy = new T.Object3D();
function box(size: T.Vector3, pos: T.Vector3, color: string) {
  const m = new T.Mesh(
    cube,
    new T.MeshStandardMaterial({ color, roughness: 0.85 }),
  );
  m.scale.copy(size);
  m.position.copy(pos);
  m.castShadow = true;
  m.receiveShadow = true;
  scene.add(m);
  return m;
}
for (const x of [-1.95, 1.95]) {
  box(new T.Vector3(0.55, 0.65, 6.3), new T.Vector3(x, 0.55, 0), "#252a27");
  for (let z = -2.5; z <= 2.5; z += 0.72) {
    const wheel = new T.Mesh(
      new T.CylinderGeometry(0.39, 0.39, 0.57, 16),
      new T.MeshStandardMaterial({
        color: "#454d3d",
        metalness: 0.3,
        roughness: 0.9,
      }),
    );
    wheel.rotation.z = Math.PI / 2;
    wheel.position.set(x, 0.6, z);
    scene.add(wheel);
  }
  for (let z = -3; z <= 3; z += 0.22)
    box(new T.Vector3(0.61, 0.06, 0.1), new T.Vector3(x, 0.9, z), "#353c33");
}
box(new T.Vector3(0.18, 0.18, 3.5), new T.Vector3(0, 2.65, 2.85), "#535b40");
box(new T.Vector3(0.9, 0.5, 0.4), new T.Vector3(0, 2.65, 1.3), "#646c4c");
// Collapse static track, wheel and barrel meshes into one vertex-colored draw.
const staticParts: T.BufferGeometry[] = [];
for (const child of [...scene.children])
  if (child instanceof T.Mesh && child !== ground) {
    child.updateMatrix();
    const geo = child.geometry.clone().applyMatrix4(child.matrix);
    const color = (child.material as T.MeshStandardMaterial).color;
    const data = new Float32Array(geo.getAttribute("position").count * 3);
    for (let i = 0; i < data.length; i += 3) {
      data[i] = color.r;
      data[i + 1] = color.g;
      data[i + 2] = color.b;
    }
    geo.setAttribute("color", new T.BufferAttribute(data, 3));
    geo.deleteAttribute("uv");
    staticParts.push(geo.toNonIndexed());
    scene.remove(child);
    (child.material as T.Material).dispose();
  }
const staticTank = new T.Mesh(
  mergeGeometries(staticParts),
  new T.MeshStandardMaterial({
    vertexColors: true,
    roughness: 0.85,
    metalness: 0.2,
  }),
);
staticTank.castShadow = true;
staticTank.receiveShadow = true;
scene.add(staticTank);
for (const geo of staticParts) geo.dispose();
const sim = new Simulation();
const voxels = new T.InstancedMesh(
  cube,
  new T.MeshStandardMaterial({ roughness: 0.83, metalness: 0.25 }),
  sim.cells.length,
);
voxels.castShadow = true;
voxels.receiveShadow = true;
voxels.instanceMatrix.setUsage(T.DynamicDrawUsage);
scene.add(voxels);
const fx = new T.InstancedMesh(cube, new T.MeshBasicMaterial(), 1200);
fx.instanceMatrix.setUsage(T.DynamicDrawUsage);
fx.frustumCulled = false;
scene.add(fx);
const smoke = new T.InstancedMesh(
  cube,
  new T.MeshBasicMaterial({
    color: "#7d8279",
    transparent: true,
    opacity: 0.16,
    depthWrite: false,
  }),
  100,
);
smoke.instanceMatrix.setUsage(T.DynamicDrawUsage);
smoke.frustumCulled = false;
scene.add(smoke);
const splats = new T.InstancedMesh(
  new T.PlaneGeometry(1, 1),
  new T.MeshBasicMaterial({
    color: "#481b17",
    side: T.DoubleSide,
    polygonOffset: true,
    polygonOffsetFactor: -2,
  }),
  400,
);
splats.instanceMatrix.setUsage(T.DynamicDrawUsage);
splats.frustumCulled = false;
scene.add(splats);
const colors = ["#727858", "#626b49", "#71634a", "#b39b7d"];
let geometryDirty = true;
function syncCells() {
  let i = 0;
  for (const c of sim.cells) {
    if (!c.alive) continue;
    dummy.position.copy(c.p);
    dummy.scale.copy(c.size);
    dummy.quaternion.identity();
    dummy.updateMatrix();
    voxels.setMatrixAt(i, dummy.matrix);
    const color = new T.Color(colors[c.kind]);
    color.multiplyScalar(0.87 + ((i * 17) % 19) / 100);
    voxels.setColorAt(i++, color);
  }
  voxels.count = i;
  voxels.instanceMatrix.needsUpdate = true;
  if (voxels.instanceColor) voxels.instanceColor.needsUpdate = true;
  voxels.computeBoundingSphere();
  geometryDirty = false;
}
const impact = new T.Vector3(-1.6, 1.45, 0),
  from = new T.Vector3(-1, 0, 0),
  normal = new T.Vector3(-1, 0, 0);
const radius = 0.85;
const sphere = new T.Mesh(
  new T.SphereGeometry(radius, 14, 8),
  new T.MeshBasicMaterial({
    color: "#e5bb77",
    wireframe: true,
    transparent: true,
    opacity: 0.1,
    depthTest: false,
  }),
);
sphere.renderOrder = 4;
scene.add(sphere);
const center = new T.Mesh(
  new T.SphereGeometry(0.055, 10, 8),
  new T.MeshBasicMaterial({ color: "#ffe3a3", depthTest: false }),
);
center.renderOrder = 5;
scene.add(center);
const handle = new T.Mesh(
  new T.SphereGeometry(0.095, 12, 8),
  new T.MeshBasicMaterial({ color: "#ffc065", depthTest: false }),
);
handle.renderOrder = 5;
scene.add(handle);
const rayLine = new T.Line(
  new T.BufferGeometry(),
  new T.LineBasicMaterial({ color: "#ffd696", depthTest: false }),
);
rayLine.renderOrder = 5;
scene.add(rayLine);
function syncAim() {
  sphere.position.copy(impact);
  center.position.copy(impact);
  handle.position.copy(impact).addScaledVector(from, radius);
  rayLine.geometry.dispose();
  rayLine.geometry = new T.BufferGeometry().setFromPoints([
    impact.clone().addScaledVector(from, 2),
    impact.clone().addScaledVector(from, -1),
  ]);
}
syncAim();
const caster = new T.Raycaster(),
  pointer = new T.Vector2();
function pick(e: PointerEvent) {
  const rect = canvas.getBoundingClientRect();
  pointer.set(
    ((e.clientX - rect.left) / rect.width) * 2 - 1,
    (-(e.clientY - rect.top) / rect.height) * 2 + 1,
  );
  caster.setFromCamera(pointer, camera);
  return caster.intersectObject(voxels)[0];
}
let drag: { x: number; y: number; time: number; aim: boolean } | undefined;
canvas.addEventListener("contextmenu", (e) => e.preventDefault());
canvas.addEventListener("pointerdown", (e) => {
  if (e.button !== 0) return;
  const hit = pick(e);
  if (hit && !caster.intersectObject(handle).length) {
    impact.copy(hit.point);
    if (hit.face) normal.copy(hit.face.normal);
    syncAim();
  }
  drag = { x: e.clientX, y: e.clientY, time: performance.now(), aim: false };
  canvas.setPointerCapture(e.pointerId);
});
canvas.addEventListener("pointermove", (e) => {
  if (!drag) return;
  if (performance.now() - drag.time < 100) return;
  const dx = e.clientX - drag.x,
    dy = e.clientY - drag.y;
  if (Math.abs(dx) + Math.abs(dy) < 1) return;
  drag.aim = true;
  const up = new T.Vector3(0, 1, 0).applyQuaternion(camera.quaternion),
    right = new T.Vector3(1, 0, 0).applyQuaternion(camera.quaternion);
  from
    .applyAxisAngle(up, -dx * 0.012)
    .applyAxisAngle(right, -dy * 0.012)
    .normalize();
  drag.x = e.clientX;
  drag.y = e.clientY;
  syncAim();
});
canvas.addEventListener("pointerup", () => {
  drag = undefined;
});
canvas.addEventListener("pointercancel", () => {
  drag = undefined;
});
let presets: Preset[] = [],
  settings: Record<string, any> = {},
  roundIndex = 12;
let paused = false;
let shotSupported = true;
const fireButton = document.querySelector<HTMLButtonElement>("#fire")!;
function applyPreset(p: Preset) {
  settings = p.settings;
  const shot = settings.shot ?? {};
  shotSupported = !shot.target?.man;
  fireButton.disabled = !shotSupported;
  roundIndex = Math.floor(finite(shot.shell, 12, 0, rounds.length - 1));
  sim.seed = finite(shot.seed, 41, 0, 4294967295);
  const point = shot.target?.point;
  if (Array.isArray(point) && point.length === 3)
    impact.set(
      finite(point[0], -1.6),
      finite(point[1], 1.45),
      finite(point[2], 0),
    );
  const az = (finite(shot.from_az, -90) * Math.PI) / 180,
    el = (finite(shot.from_el, 0, -90, 90) * Math.PI) / 180;
  from.set(
    Math.sin(az) * Math.cos(el),
    Math.sin(el),
    Math.cos(az) * Math.cos(el),
  ); // Find the actual visible entry along the saved target ray; interior targets remain useful.
  if (geometryDirty) syncCells();
  caster.set(impact.clone().addScaledVector(from, 20), from.clone().negate());
  const entry = caster.intersectObject(voxels)[0];
  if (entry) {
    if (entry.face) normal.copy(entry.face.normal);
  } else normal.copy(from);
  syncAim();
  const eye = settings.camera?.eye;
  if (Array.isArray(eye) && eye.length === 3) {
    camera.position.set(
      finite(eye[0], -9, -30, 30),
      finite(eye[1], 6, 0.3, 30),
      finite(eye[2], 10, -30, 30),
    );
    controls.update();
  }
  const exposure = settings.sky?.exposure;
  if (typeof exposure === "number")
    renderer.toneMappingExposure = finite(exposure, 1.15, 0.2, 3);
  status.textContent = shotSupported
    ? `${p.name} · Coarse JS model · fixed native tuning`
    : `${p.name} · Named crew target (${shot.target.man}) unsupported; select a point-target shot`;
}
async function load() {
  try {
    const response = await fetch("/native-preset.json");
    if (!response.ok) throw Error("preset unavailable");
    presets = parsePresets(await response.json());
    if (!presets.length) throw Error("no valid presets");
    const options = shotOptions(presets);
    presets = options.choices;
    for (const [i, p] of presets.entries())
      shotSelect.add(new Option(p.name, `p:${i}`));
    if (!options.authored)
      for (const [i, r] of rounds.entries())
        shotSelect.add(new Option(r.name, `r:${i}`));
    if (!presets.length) {
      shotSupported = false;
      fireButton.disabled = true;
      status.textContent =
        "Native authored shot list is empty; add a shot in the native editor and export again.";
      return;
    }
    shotSelect.value = `p:${options.selected}`;
    applyPreset(presets[options.selected]);
  } catch (e) {
    status.textContent = `Preset load failed: ${e}. Using baseline.`;
    for (const [i, r] of rounds.entries())
      shotSelect.add(new Option(r.name, `r:${i}`));
    shotSelect.value = "r:12";
  }
}
shotSelect.onchange = () => {
  const [type, index] = shotSelect.value.split(":");
  if (type === "p") applyPreset(presets[+index]);
  else {
    roundIndex = +index;
    status.textContent = `${rounds[roundIndex].name} · fixed native tuning`;
  }
};
function fire() {
  if (!shotSupported) return;
  // Keep the authored marker exact; resolve entry separately for this coarse hull.
  if (geometryDirty) syncCells();
  caster.set(impact.clone().addScaledVector(from, 20), from.clone().negate());
  const entry = caster.intersectObject(voxels)[0];
  if (entry?.face) normal.copy(entry.face.normal);
  const r = rounds[roundIndex];
  const result = sim.fire(
    entry?.point ?? impact,
    from.clone().negate(),
    normal,
    r,
    finite(settings.shot?.range_m, 400, 0, 6000),
    finite(settings.tissue?.active_reach_m, 0.55, 0.1, 1),
    finite(settings.tissue?.spread, 1, 0.1, 3),
  );
  geometryDirty = true;
  status.textContent = `${result.passed ? "PENETRATION" : "STOPPED"} · ${result.pen.toFixed(0)} / ${result.effective.toFixed(0)} mm · ${result.lost} cells · ${result.crew} crew voxels${r.caliber_mm >= 800 ? " · experimental approximation" : ""}`;
}
document.querySelector<HTMLButtonElement>("#fire")!.onclick = fire;
addEventListener("keydown", (e) => {
  if ((e.target as HTMLElement).matches("input,select,textarea") || e.repeat)
    return;
  if (e.code === "KeyF") fire();
  if (e.code === "Backspace") {
    e.preventDefault();
    sim.reset();
    geometryDirty = true;
  }
  if (e.code === "Space") {
    e.preventDefault();
    paused = !paused;
    status.textContent = paused ? "Paused · Space resumes" : "Live · F fires";
  }
});
addEventListener("resize", () => {
  camera.aspect = innerWidth / innerHeight;
  camera.updateProjectionMatrix();
  renderer.setSize(innerWidth, innerHeight);
});
canvas.addEventListener("webglcontextlost", (e) => {
  e.preventDefault();
  status.textContent = "Graphics context lost. Reload the page to restore.";
  running = false;
});
canvas.addEventListener("webglcontextrestored", () => location.reload());
let last = performance.now(),
  accumulator = 0,
  frames: number[] = [],
  running = true,
  dropped = 0;
const palette = ["#797860", "#ffbf5e", "#7b201c", "#72766b"];
function frame(now: number) {
  if (!running) return;
  const elapsed = Math.min(0.1, (now - last) / 1000);
  last = now;
  frames.push(elapsed * 1000);
  if (frames.length > 600) frames.shift();
  if (!paused) {
    accumulator += elapsed;
    const dt = Number(stepSelect.value);
    let work = 0;
    while (accumulator >= dt && work < 8) {
      sim.step(dt);
      accumulator -= dt;
      work++;
    }
    if (accumulator >= dt) {
      dropped++;
      accumulator = 0;
    }
  }
  if (geometryDirty) syncCells();
  controls.update();
  let solidCount = 0,
    smokeCount = 0;
  for (const p of sim.particles) {
    dummy.position.copy(p.p);
    dummy.scale.setScalar(p.size * (p.kind === 3 ? 1 + (4 - p.life) * 0.6 : 1));
    dummy.quaternion.identity();
    dummy.updateMatrix();
    if (p.kind === 3) {
      if (smokeCount < 100) smoke.setMatrixAt(smokeCount++, dummy.matrix);
    } else {
      fx.setMatrixAt(solidCount, dummy.matrix);
      fx.setColorAt(solidCount++, new T.Color(palette[p.kind]));
    }
  }
  fx.count = solidCount;
  smoke.count = smokeCount;
  smoke.instanceMatrix.needsUpdate = true;
  fx.instanceMatrix.needsUpdate = true;
  if (fx.instanceColor) fx.instanceColor.needsUpdate = true;
  for (let i = 0; i < sim.splats.length; i++) {
    const s = sim.splats[i];
    dummy.position.copy(s.p);
    dummy.scale.setScalar(s.size);
    dummy.quaternion.setFromUnitVectors(new T.Vector3(0, 0, 1), s.n);
    dummy.updateMatrix();
    splats.setMatrixAt(i, dummy.matrix);
  }
  splats.count = sim.splats.length;
  splats.instanceMatrix.needsUpdate = true;
  renderer.render(scene, camera);
  if (frames.length % 20 === 0)
    document.querySelector("#metrics")!.textContent =
      `${(1000 / (frames.reduce((a, b) => a + b, 0) / frames.length)).toFixed(0)} FPS · ${renderer.info.render.calls} draws · ${sim.particles.length}/1200 FX`;
  requestAnimationFrame(frame);
}
// Read-only diagnostics for QA; no simulation/frame cache or automatic capture.
Object.assign(window, {
  panzerQA: {
    snapshot: () => ({
      shots: sim.shots,
      cells: voxels.count,
      removed: sim.removed,
      particles: sim.particles.length,
      splats: sim.splats.length,
      simTime: sim.time,
      dropped,
      drawCalls: renderer.info.render.calls,
      triangles: renderer.info.render.triangles,
      frames: [...frames],
      impact: impact.toArray(),
      from: from.toArray(),
      camera: camera.position.toArray(),
      preset: presets[0]?.name,
    }),
    project: (p: number[]) => {
      const v = new T.Vector3(...(p as [number, number, number])).project(
        camera,
      );
      return {
        x: ((v.x + 1) * innerWidth) / 2,
        y: ((1 - v.y) * innerHeight) / 2,
      };
    },
    stop: () => {
      running = false;
      renderer.dispose();
    },
  },
});
load();
requestAnimationFrame(frame);
