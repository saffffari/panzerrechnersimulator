import * as T from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { mergeGeometries } from "three/addons/utils/BufferGeometryUtils.js";
import { Battle, road, obstacles, type Point } from "./battle";
import type { Round } from "./preset";
// Render adapter only. Enemy intentions and remembered knowledge belong to Battle/Squad.
export class PlayView {
  scene = new T.Scene();
  camera = new T.PerspectiveCamera(48, innerWidth / innerHeight, 0.1, 1500);
  controls: OrbitControls;
  battle = new Battle();
  aim: Point = { ...road.sample(100) };
  active = false;
  private dummy = new T.Object3D();
  private enemies: T.InstancedMesh[] = [];
  private player = new T.Group();
  private marker: T.Mesh;
  private traces: T.Line[] = [];
  private previousPlayer = { x: 0, z: 0 };
  constructor(canvas: HTMLCanvasElement) {
    this.scene.background = new T.Color("#bbc9cf");
    this.scene.fog = new T.Fog("#bbc9cf", 180, 540);
    this.scene.add(new T.HemisphereLight("#e9f2ff", "#5c6567", 2.6));
    const sun = new T.DirectionalLight("#ffe7cc", 2);
    sun.position.set(-100, 180, 40);
    this.scene.add(sun);
    const floor = this.ribbon(road.halfWidth, "#c2c9c7", 0.02);
    floor.receiveShadow = false;
    this.scene.add(floor);
    this.scene.add(this.ribbon(9, "#737b79", 0.04));
    const cliffParts: T.BufferGeometry[] = [];
    for (let s = road.min + 8; s < road.max; s += 22)
      for (const side of [-1, 1]) {
        const p = road.sample(s, side * (36 + Math.sin(s * 0.013) * 3));
        const h = 17 + (Math.sin(s * 0.09 + side) + 1) * 12;
        const geo = new T.DodecahedronGeometry(1, 0);
        geo.scale(13, h, 15);
        geo.rotateY(s);
        geo.translate(p.x, h * 0.55, p.z);
        cliffParts.push(geo.toNonIndexed());
      }
    for (const o of obstacles) {
      const geo = new T.DodecahedronGeometry(o.radius, 0);
      geo.scale(1, 0.8, 1);
      geo.translate(o.x, o.radius * 0.55, o.z);
      cliffParts.push(geo.toNonIndexed());
    }
    const cliffs = new T.Mesh(
      mergeGeometries(cliffParts),
      new T.MeshStandardMaterial({
        color: "#6e797c",
        roughness: 1,
        flatShading: true,
      }),
    );
    this.scene.add(cliffs);
    for (const g of cliffParts) g.dispose();
    // Low-poly snow caps and road markings communicate the confined, winter course.
    const markings = new T.InstancedMesh(
      new T.BoxGeometry(0.12, 0.03, 3),
      new T.MeshBasicMaterial({ color: "#b6b9aa" }),
      60,
    );
    let markingsCount = 0;
    for (let s = 0; s < road.max; s += 20) {
      const p = road.sample(s);
      this.dummy.position.set(p.x, 0.06, p.z);
      this.dummy.rotation.set(0, p.heading, 0);
      this.dummy.scale.setScalar(1);
      this.dummy.updateMatrix();
      markings.setMatrixAt(markingsCount++, this.dummy.matrix);
    }
    markings.count = markingsCount;
    this.scene.add(markings);
    const parts: [T.BufferGeometry, T.Vector3, T.Vector3, string][] = [
      [
        new T.BoxGeometry(1, 1, 1),
        new T.Vector3(3, 1.05, 5.5),
        new T.Vector3(0, 1.05, 0),
        "#566449",
      ],
      [
        new T.CylinderGeometry(1.0, 1.3, 0.9, 8),
        new T.Vector3(1, 1, 1),
        new T.Vector3(0, 2, 0),
        "#637653",
      ],
      [
        new T.BoxGeometry(1, 1, 1),
        new T.Vector3(0.14, 0.14, 3.4),
        new T.Vector3(0, 2.12, 2.5),
        "#3e4b36",
      ],
      [
        new T.BoxGeometry(1, 1, 1),
        new T.Vector3(0.5, 0.75, 5.9),
        new T.Vector3(-1.65, 0.55, 0),
        "#242e2b",
      ],
      [
        new T.BoxGeometry(1, 1, 1),
        new T.Vector3(0.5, 0.75, 5.9),
        new T.Vector3(1.65, 0.55, 0),
        "#242e2b",
      ],
    ];
    for (const [geometry, scale, position, color] of parts) {
      const mesh = new T.InstancedMesh(
        geometry,
        new T.MeshStandardMaterial({ color, roughness: 0.9 }),
        10,
      );
      mesh.instanceMatrix.setUsage(T.DynamicDrawUsage);
      mesh.frustumCulled = false;
      mesh.userData = { scale, position };
      this.enemies.push(mesh);
      this.scene.add(mesh);
      const tankPart = new T.Mesh(
        geometry,
        new T.MeshStandardMaterial({
          color: color === "#242e2b" ? color : "#97866a",
          roughness: 0.9,
        }),
      );
      tankPart.scale.copy(scale);
      tankPart.position.copy(position);
      this.player.add(tankPart);
    }
    this.scene.add(this.player);
    this.marker = new T.Mesh(
      new T.RingGeometry(0.5, 0.7, 24),
      new T.MeshBasicMaterial({
        color: "#ffcd87",
        side: T.DoubleSide,
        depthTest: false,
      }),
    );
    this.marker.rotation.x = -Math.PI / 2;
    this.marker.renderOrder = 3;
    this.scene.add(this.marker);
    for (let i = 0; i < 32; i++) {
      const line = new T.Line(
        new T.BufferGeometry().setFromPoints([
          new T.Vector3(),
          new T.Vector3(),
        ]),
        new T.LineBasicMaterial({
          color: "#ffc578",
          transparent: true,
          opacity: 0.85,
        }),
      );
      line.visible = false;
      this.traces.push(line);
      this.scene.add(line);
    }
    this.controls = new OrbitControls(this.camera, canvas);
    this.controls.enabled = false;
    this.controls.enableDamping = true;
    this.controls.mouseButtons = {
      LEFT: null as any,
      MIDDLE: T.MOUSE.PAN,
      RIGHT: T.MOUSE.ROTATE,
    };
    this.controls.minDistance = 8;
    this.controls.maxDistance = 90;
    this.controls.maxPolarAngle = Math.PI * 0.48;
    this.reset(5);
  }
  private ribbon(width: number, color: string, y: number) {
    const verts: number[] = [],
      indices: number[] = [];
    for (let i = 0; i < road.points.length; i++) {
      const s = road.points[i][0];
      for (const side of [-1, 1]) {
        const p = road.sample(s, side * width);
        verts.push(p.x, y, p.z);
      }
      if (i) {
        const a = (i - 1) * 2;
        indices.push(a, a + 2, a + 1, a + 1, a + 2, a + 3);
      }
    }
    const geo = new T.BufferGeometry();
    geo.setAttribute("position", new T.Float32BufferAttribute(verts, 3));
    geo.setIndex(indices);
    geo.computeVertexNormals();
    return new T.Mesh(
      geo,
      new T.MeshStandardMaterial({ color, roughness: 1, side: T.DoubleSide }),
    );
  }
  reset(count: number) {
    this.battle.restart(count);
    this.aim = { ...road.sample(100) };
    this.previousPlayer = { x: 0, z: 0 };
    this.camera.position.set(-10, 10, 18);
    this.controls.target.set(0, 1.5, -5);
    this.controls.update();
    this.sync();
  }
  setActive(active: boolean) {
    this.active = active;
    this.controls.enabled = active;
  }
  pick(clientX: number, clientY: number, canvas: HTMLCanvasElement) {
    const rect = canvas.getBoundingClientRect(),
      ray = new T.Raycaster();
    ray.setFromCamera(
      new T.Vector2(
        ((clientX - rect.left) / rect.width) * 2 - 1,
        (-(clientY - rect.top) / rect.height) * 2 + 1,
      ),
      this.camera,
    );
    const hits = ray.intersectObjects(this.enemies);
    const hit = hits.find(
      (h) =>
        h.instanceId !== undefined &&
        this.battle.squad.enemies[h.instanceId]?.hp > 0,
    );
    if (hit && hit.instanceId !== undefined) {
      const e = this.battle.squad.enemies[hit.instanceId];
      this.aim = { x: e.x, z: e.z };
      return;
    }
    const point = new T.Vector3();
    if (
      ray.ray.intersectPlane(new T.Plane(new T.Vector3(0, 1, 0), 0), point) &&
      road.clear({ x: point.x, z: point.z }, 0)
    )
      this.aim = { x: point.x, z: point.z };
  }
  fire(r: Round) {
    return this.battle.fire(this.aim, r);
  }
  sync() {
    const p = this.battle.player;
    const dx = p.x - this.previousPlayer.x,
      dz = p.z - this.previousPlayer.z;
    this.camera.position.x += dx;
    this.camera.position.z += dz;
    this.controls.target.x += dx;
    this.controls.target.z += dz;
    this.previousPlayer = { x: p.x, z: p.z };
    this.player.position.set(p.x, 0, p.z);
    this.player.rotation.y = p.heading;
    for (const mesh of this.enemies) {
      mesh.count = this.battle.squad.enemies.length;
      for (const e of this.battle.squad.enemies) {
        const turret = mesh === this.enemies[1] || mesh === this.enemies[2],
          heading = turret
            ? Math.atan2(
                this.battle.squad.memory.point.x - e.x,
                this.battle.squad.memory.point.z - e.z,
              )
            : e.heading;
        const offset = (mesh.userData.position as T.Vector3)
          .clone()
          .applyAxisAngle(new T.Vector3(0, 1, 0), heading);
        this.dummy.position.set(e.x + offset.x, offset.y, e.z + offset.z);
        this.dummy.rotation.set(0, heading, e.hp <= 0 ? 0.07 : 0);
        this.dummy.scale.copy(mesh.userData.scale);
        this.dummy.updateMatrix();
        mesh.setMatrixAt(e.id, this.dummy.matrix);
        mesh.setColorAt(e.id, new T.Color(e.hp <= 0 ? "#343835" : "#ffffff"));
      }
      mesh.instanceMatrix.needsUpdate = true;
      if (mesh.instanceColor) mesh.instanceColor.needsUpdate = true;
    }
    this.marker.position.set(this.aim.x, 0.12, this.aim.z);
    const events = this.battle.events.splice(0);
    for (const e of events) {
      const line = this.traces.find((l) => !l.visible) ?? this.traces[0];
      line.geometry.dispose();
      line.geometry = new T.BufferGeometry().setFromPoints([
        new T.Vector3(e.from.x, 2, e.from.z),
        new T.Vector3(e.to.x, 1.8, e.to.z),
      ]);
      (line.material as T.LineBasicMaterial).color.set(
        e.player ? "#ffde94" : "#ef905b",
      );
      line.visible = true;
      line.userData.expires = this.battle.time + 0.18;
    }
    for (const line of this.traces)
      if (line.userData.expires < this.battle.time) line.visible = false;
    this.controls.update();
  }
  resize() {
    this.camera.aspect = innerWidth / innerHeight;
    this.camera.updateProjectionMatrix();
  }
  hud() {
    return `Hull ${this.battle.player.hp}% · T-34s ${this.battle.squad.enemies.filter((e) => e.hp > 0).length}/${this.battle.squad.enemies.length} · ${this.battle.outcome} · ${Math.abs(this.battle.player.speed * 3.6).toFixed(0)} km/h`;
  }
}
