import * as T from "three";
import { GLTFLoader } from "three/addons/loaders/GLTFLoader.js";
import { MeshoptDecoder } from "three/addons/libs/meshopt_decoder.module.js";
export const SCALE = 35 / 38.919815;
export type DamageLevel = 0 | 1 | 2;
export class NativeTiger {
  root = new T.Group();
  turret = new T.Group();
  gun = new T.Group();
  meshes: T.Mesh[] = [];
  crew: T.Group[] = [];
  lamps: T.PointLight[] = [];
  statuses = new Map<string, DamageLevel>();
  roof = false;
  section = new T.Plane(new T.Vector3(0, -1, 0), 100);
  sources = new Map<T.Material, T.Color>();
  async load() {
    const loader = new GLTFLoader().setMeshoptDecoder(MeshoptDecoder);
    const [h, t, g, ...crew] = await Promise.all(
      [
        "tiger-hull",
        "tiger-turret",
        "tiger-gun",
        "crew-1",
        "crew-2",
        "crew-3",
        "crew-4",
        "crew-5",
      ].map((n) => loader.loadAsync(`/native/${n}.glb`)),
    );
    this.root.scale.setScalar(SCALE);
    this.root.add(h.scene, this.turret);
    this.turret.position.set(0, 0.524, 0.0192);
    this.turret.add(t.scene, this.gun);
    this.gun.position.set(0.0044, 1.8456, 1.0396);
    this.gun.add(g.scene);
    // These transforms come verbatim from game.rs::crew_live_model, permanent face order.
    const seats = [
      ["Commander", 0.564, 0.743, -0.423, 0, 1.08],
      ["Gunner", 0.5, 0.82, 0.25, 0, 0.78],
      ["Driver", 0.535, 0.675745, 1.777748, 0, 0.8],
      ["Loader", -0.72, 0.72, 0.12, Math.PI / 2, 0.82],
      ["Radio", -0.635, 0.635, 1.58, 0, 0.8],
    ] as const;
    crew.forEach((asset, i) => {
      const [role, x, y, z, yaw, s] = seats[i],
        group = new T.Group();
      group.name = role;
      group.userData.module = role;
      group.position.set(x, y, z);
      group.rotation.y = yaw;
      group.scale.setScalar(s);
      group.add(asset.scene);
      (i === 2 || i === 4 ? this.root : this.turret).add(group);
      this.crew.push(group);
      asset.scene.traverse((o) => (o.userData.module = role));
    });
    const [hatch, helmet] = await Promise.all(
      ["hatch", "helmet"].map((n) => loader.loadAsync(`/native/${n}.glb`)),
    );
    hatch.scene.position.set(0.963, 2.614, -0.333);
    hatch.scene.userData.roof = true;
    hatch.scene.userData.module = "TurretTraverse";
    hatch.scene.traverse((o) => {
      if (o instanceof T.Mesh)
        o.material = new T.MeshStandardMaterial({
          color: "#696c73",
          metalness: 0.15,
          roughness: 0.5,
        });
    });
    this.turret.add(hatch.scene);
    const bounds = new T.Box3().setFromObject(helmet.scene),
      pivot = new T.Vector3(
        (bounds.min.x + bounds.max.x) / 2,
        bounds.max.y - 0.2 * (bounds.max.y - bounds.min.y),
        (bounds.min.z + bounds.max.z) / 2,
      );
    helmet.scene.traverse((o) => {
      if (o instanceof T.Mesh) {
        o.geometry = o.geometry.clone();
        o.geometry.translate(-pivot.x, -pivot.y, -pivot.z);
        o.geometry.scale(1.25, 1.25, 1.25);
        o.geometry.translate(pivot.x, pivot.y, pivot.z);
      }
    });
    for (let i = 0; i < 5; i++) {
      const copy = helmet.scene.clone(true);
      copy.position.set(0.01734, i === 0 ? -0.002827 : -0.22517, 0.016529);
      copy.rotation.x = -Math.PI / 180;
      copy.traverse((o) => (o.userData.module = this.crew[i].name));
      this.crew[i].add(copy);
    }
    // Closed overlapping bearing collar: a 128-segment annulus, never a disk across the cabin.
    const seal = new T.Mesh(
      new T.LatheGeometry(
        [
          new T.Vector2(1.17, 1.91),
          new T.Vector2(1.285, 1.91),
          new T.Vector2(1.285, 2.055),
          new T.Vector2(1.17, 2.055),
          new T.Vector2(1.17, 1.91),
        ],
        128,
      ),
      new T.MeshStandardMaterial({
        color: "#696c73",
        metalness: 0.35,
        roughness: 0.65,
      }),
    );
    seal.userData.module = "TurretTraverse";
    seal.name = "Overlapping turret bearing";
    this.root.add(seal);
    this.root.traverse((o) => {
      if (!(o instanceof T.Mesh)) return;
      this.meshes.push(o);
      o.castShadow = o.receiveShadow = true;
      let p: T.Object3D | null = o;
      while (p) {
        if (p.userData.module) {
          o.userData.module = p.userData.module;
          break;
        }
        p = p.parent;
      }
      p = o;
      while (p) {
        if (p.userData.roof) {
          o.userData.roof = true;
          break;
        }
        p = p.parent;
      }
      const multi = Array.isArray(o.material);
      const mapped = (multi ? o.material : ([o.material] as any[])).map(
        (m: T.Material) => {
          const a = m.clone() as T.MeshStandardMaterial;
          a.side = T.DoubleSide;
          a.clippingPlanes = [this.section];
          this.sources.set(a, a.color.clone());
          return a;
        },
      );
      o.material = multi ? mapped : mapped[0];
    });
    for (const [parent, x, y, z] of [
      [this.root, 0.68, 1.88, 1.75],
      [this.root, -0.68, 1.88, 1.75],
      [this.turret, -0.01265, 2.19, -0.0022],
    ] as const) {
      const light = new T.PointLight("#ff9130", 1.8, 4, 2);
      light.position.set(x, y, z);
      light.castShadow = true;
      light.shadow.mapSize.set(256, 256);
      light.shadow.bias = -0.0002;
      light.shadow.normalBias = 0.003;
      light.shadow.camera.near = 0.03;
      light.shadow.camera.far = 5;
      parent.add(light);
      this.lamps.push(light);
      const lens = new T.Mesh(
        new T.BoxGeometry(0.028, 0.028, 0.028),
        new T.MeshBasicMaterial({ color: "#ffb35f" }),
      );
      lens.position.copy(light.position);
      parent.add(lens);
    }
    this.root.updateMatrixWorld(true);
  }
  setRoof(cut: boolean) {
    this.roof = cut;
    for (const m of this.meshes) m.visible = !(cut && m.userData.roof);
  }
  setLamps(on: boolean, gain = 1) {
    for (const l of this.lamps) l.intensity = on ? 1.8 * gain : 0;
  }
  setStatus(module: string, level: DamageLevel) {
    this.statuses.set(module, level);
  }
  tintDamage(on: boolean) {
    for (const mesh of this.meshes) {
      const state = this.statuses.get(mesh.userData.module) || 0;
      for (const mat of (Array.isArray(mesh.material)
        ? mesh.material
        : [mesh.material]) as T.MeshStandardMaterial[]) {
        mat.color.copy(this.sources.get(mat)!);
        mat.emissive.set(0);
        if (on && state) {
          mat.color.set(state === 1 ? "#ff8b21" : "#df2925");
          mat.emissive.copy(mat.color).multiplyScalar(0.25);
        }
      }
    }
  }
  reset() {
    this.statuses.clear();
    this.turret.rotation.set(0, 0, 0);
    this.gun.rotation.set(0, 0, 0);
    this.tintDamage(false);
  }
}
