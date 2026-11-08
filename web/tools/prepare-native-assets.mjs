import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { NodeIO } from "@gltf-transform/core";
import { ALL_EXTENSIONS } from "@gltf-transform/extensions";
import {
  dedup,
  weld,
  simplify,
  flatten,
  join,
  prune,
  getBounds,
  meshopt,
} from "@gltf-transform/functions";
import { MeshoptSimplifier, MeshoptEncoder } from "meshoptimizer";
const source = "/windows/shop/panzerrechnersystem/assets";
const native = "/home/alex/panzer/voxel-runtime/src";
const out = path.resolve("public/native");
fs.mkdirSync(out, { recursive: true });
const io = new NodeIO()
  .registerExtensions(ALL_EXTENSIONS)
  .registerDependencies({ "meshopt.encoder": MeshoptEncoder });
await MeshoptSimplifier.ready;
await MeshoptEncoder.ready;
const text = fs.readFileSync(native + "/tiger_exterior_finishes.rs", "utf8");
const names = (s, key) =>
  new Set(
    [
      ...s
        .split(`const ${key}:`)[1]
        .split("];")[0]
        .matchAll(/"([^"]+)"/g),
    ].map((x) => x[1]),
  );
const paint = names(text, "PAINTED"),
  tracks = names(text, "TRACK_STEEL"),
  cables = names(text, "CABLE_EXHAUST_STEEL");
const extra = names(
  fs.readFileSync(native + "/tank.rs", "utf8"),
  "TIGER_PAINTED_NODES",
);
const lin = (v) => {
  v /= 255;
  return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
};
const manifest = {
  source: "Native Tiger baked assemblies and authored AdrielCZ crew",
  nativeCommit: "9fc6553",
  frameScale: 35 / 38.919815,
  files: [],
  parts: [],
};
for (const part of ["hull", "turret", "gun"]) {
  const file = `tanks/tiger/baked/tiger_${part}.glb`,
    doc = await io.read(source + "/" + file),
    mats = new Map();
  const material = (key, rgb, metal, rough) => {
    if (!mats.has(key))
      mats.set(
        key,
        doc
          .createMaterial(key)
          .setBaseColorFactor([...rgb.map(lin), 1])
          .setMetallicFactor(metal)
          .setRoughnessFactor(rough),
      );
    return mats.get(key);
  };
  let inputTriangles = 0;
  for (const node of doc.getRoot().listNodes()) {
    const mesh = node.getMesh();
    if (!mesh) continue;
    const name = node.getName(),
      bounds = getBounds(node),
      c = bounds.min.map((v, i) => (v + bounds.max[i]) / 2);
    let group = part;
    if (tracks.has(name) && name.startsWith("Hull_"))
      group = c[0] > 0 ? "LeftTrack" : "RightTrack";
    else if (part === "gun") group = "MainGun";
    else if (part === "hull")
      group =
        c[2] < -1.0
          ? "Engine"
          : c[2] > 2
            ? "Transmission"
            : Math.abs(c[0]) > 1.15
              ? "AmmoRack"
              : "Hull";
    else group = "TurretTraverse";
    const roof =
      part === "hull"
        ? bounds.min[1] > 1.89 ||
          [
            "Hull_0076",
            "Hull_0708",
            "Hull_0773",
            "Hull_0780",
            "Hull_0975",
          ].includes(name)
        : part === "turret"
          ? bounds.min[1] > 2.3 || name === "Turret_0105"
          : false;
    if (name === "Hull_0355") group = "Radio";
    for (const p of mesh.listPrimitives()) {
      inputTriangles +=
        (p.getIndices()?.getCount() ?? p.getAttribute("POSITION").getCount()) /
        3;
      const m = p.getMaterial(),
        mn = m?.getName();
      if (mn === "Interior Paint")
        p.setMaterial(material("Interior white", [221, 216, 206], 0, 0.62));
      else if (mn === "Turret Floor")
        p.setMaterial(material("Basket floor", [93, 93, 91], 0, 0.84));
      else if (
        mn === "Illuminated Dial" &&
        name !== "turret position indicator backing"
      )
        p.setMaterial(material("Dial enamel", [40, 40, 40], 0, 0.45));
      else if (mn === "Material.001") {
        if (["Hull_0355", "Hull_0240"].includes(name))
          p.setMaterial(material("Cabinet enamel", [196, 194, 188], 0, 0.55));
        else if (tracks.has(name))
          p.setMaterial(material("Track steel", [58, 58, 56], 0.48, 0.9));
        else if (cables.has(name))
          p.setMaterial(material("Cable steel", [73, 72, 68], 0.6, 0.82));
        else if (paint.has(name) || extra.has(name))
          p.setMaterial(material("Exterior grey", [105, 108, 115], 0.15, 0.5));
      }
    }
    node.setExtras({ module: group, roof, frame: part });
    manifest.parts.push({
      name,
      frame: part,
      module: group,
      roof,
      min: bounds.min,
      max: bounds.max,
    });
  }
  await doc.transform(
    weld(),
    simplify({
      simplifier: MeshoptSimplifier,
      ratio: 0.12,
      error: 0.008,
      lockBorder: false,
    }),
    flatten(),
  );
  const keys = [
    ...new Set(
      doc
        .getRoot()
        .listNodes()
        .filter((n) => n.getMesh())
        .map((n) => JSON.stringify(n.getExtras())),
    ),
  ];
  for (const key of keys)
    await doc.transform(
      join({
        filter: (n) => !!n.getMesh() && JSON.stringify(n.getExtras()) === key,
      }),
    );
  await doc.transform(dedup(), prune());
  for (const node of doc
    .getRoot()
    .listNodes()
    .filter((n) => n.getMesh()))
    node.setName(
      `${part}-${node.getExtras().module}-${node.getExtras().roof ? "roof" : "body"}`,
    );
  await doc.transform(meshopt({ encoder: MeshoptEncoder, level: "medium" }));
  const dest = `tiger-${part}.glb`;
  await io.write(out + "/" + dest, doc);
  const triangles = doc
    .getRoot()
    .listMeshes()
    .reduce(
      (sum, m) =>
        sum +
        m
          .listPrimitives()
          .reduce(
            (s, p) =>
              s +
              (p.getIndices()?.getCount() ??
                p.getAttribute("POSITION").getCount()) /
                3,
            0,
          ),
      0,
    );
  manifest.files.push({
    file: dest,
    source: file,
    sha256: crypto
      .createHash("sha256")
      .update(fs.readFileSync(source + "/" + file))
      .digest("hex"),
    inputTriangles,
    triangles,
    bytes: fs.statSync(out + "/" + dest).size,
  });
  console.log(manifest.files.at(-1));
}
for (let i = 1; i <= 5; i++) {
  const pose = i === 1 ? "standing" : "seated";
  const file = `flavor/Characters/german-panzertruppe-adrielcz-cc-by-4/baked/german_panzertruppe_face0${i}_${pose}_static.glb`;
  fs.copyFileSync(source + "/" + file, out + `/crew-${i}.glb`);
  manifest.files.push({
    file: `crew-${i}.glb`,
    source: file,
    sha256: crypto
      .createHash("sha256")
      .update(fs.readFileSync(source + "/" + file))
      .digest("hex"),
  });
}
for (const [src, dst] of [
  ["tanks/tiger/baked/tiger_hatch.glb", "hatch.glb"],
  [
    "flavor/Characters/german-ww2-soldier-cc0/baked/german_steel_helmet_panzertruppe_relaxed_static.glb",
    "helmet.glb",
  ],
]) {
  fs.copyFileSync(source + "/" + src, out + "/" + dst);
  manifest.files.push({
    file: dst,
    source: src,
    sha256: crypto
      .createHash("sha256")
      .update(fs.readFileSync(source + "/" + src))
      .digest("hex"),
  });
}
fs.copyFileSync(
  source + "/tanks/tiger/baked/tiger_interior.json",
  out + "/interior.json",
);
fs.writeFileSync(out + "/manifest.json", JSON.stringify(manifest, null, 2));

fs.copyFileSync(source + "/flavor/Characters/german-panzertruppe-adrielcz-cc-by-4/LICENSE.txt", out + "/CREW-LICENSE.txt");
