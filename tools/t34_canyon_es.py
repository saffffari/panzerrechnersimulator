#!/usr/bin/env python3
"""Evolve the T-34 policy (KSL2, 31 inputs x 27 hidden) on the Canyon. docs/T34_EYES.md.

Every episode is a fresh `t34-canyon-eval SEED SECONDS` process (one policy, one seed), the same
world a `canyon-ai-run` receipt starts from; several run in parallel, niced. Generation zero is
`t34-seed` (the July academy net with hand-wired reflex units). Mutation is small on the July
weights (the manner the owner liked) and larger on everything that reads the new inputs.

Fitness per episode (the T-34 side, five tanks, 45 mm guns off):
  + 40 per penetration of the Tiger, 15 per side/rear hit, 4 per frontal hit
  + 4 per side/rear shot, 1 per frontal shot
  + 0.25 per tank-second in contact (in the fight, not hiding from it forever)
  + closing distance (sum over tanks, capped at 1500 m) / 30
  + 0.4 per tank-second off the Tiger's front quarter within 500 m (the flank)
  + 0.5 per tank-second within 800 m weighted by how far round from its bow (0 ahead, 1 astern)
  - 0.6 per tank-second the Tiger could see a tank inside its frontal 60 degrees (hide)
  + 0.05 per degree of the mean widest angle two tanks held around the Tiger (the pincer)
  - 2 per pinned tank-second (driving without moving over 1.5 s), - 0.5 per other stuck
    tank-second (throttle see-sawing through zero), - 10 per T-34 lost, + 30 if the Tiger is
    knocked out
  - 150 x mean squared difference from the July net's answers on a bank of open-ground
    observations with the Tiger in sight (keep the manner where walls are not the question)

Deterministic: numpy's generator is seeded from (base seed, generation, child); the simulator is
deterministic per (policy, seed). Usage:
  tools/t34_canyon_es.py --bin BIN --init gen0.bin --july academy.bin --out DIR [--gens 10]
"""
import argparse, concurrent.futures as cf, hashlib, json, os, struct, subprocess, sys, threading, time
import numpy as np

OBS, HID, ACT = 31, 27, 3
JULY_OBS, JULY_HID = 14, 16
GENOME = OBS * HID + HID + HID * ACT + ACT
ENV = {
    "KESSEL_ASSET_ROOT": "/windows/shop/panzerrechnersystem",
    "KESSEL_CANYON_MANIFEST": "/windows/shop/panzerrechnersystem/assets/worldgen/workshops/gaea-canyon-run-v2/manifest.v1.json",
    "KESSEL_CANYON_V2": "1", "KESSEL_CANYON_T34S": "5", "KESSEL_CANYON_GUNS": "0",
}


def load(path):
    b = open(path, "rb").read()
    magic = b[:4]
    obs, hid, act, _gid = struct.unpack("<4I", b[4:20])
    w = np.frombuffer(b[24:], dtype="<f4").astype(np.float32)
    assert act == ACT and len(w) == obs * hid + hid + hid * act + act, path
    return magic, obs, hid, w


def save(path, w, gid, fitness):
    assert len(w) == GENOME
    with open(path, "wb") as f:
        f.write(b"KSL2" + struct.pack("<4I", OBS, HID, ACT, gid) + struct.pack("<f", fitness))
        f.write(np.asarray(w, dtype="<f4").tobytes())


def outputs(w, bank, n, h):
    W1 = w[: n * h].reshape(h, n)
    b1 = w[n * h : n * h + h]
    W2 = w[n * h + h : n * h + h + h * ACT].reshape(ACT, h)
    b2 = w[n * h + h + h * ACT :]
    hid = np.tanh(bank[:, :n] @ W1.T + b1)
    return np.tanh(hid @ W2.T + b2)


def open_ground_bank(rng, count=512):
    """Tiger in sight on open ground: July slots as executor_obs fills them, the new slots clear."""
    o = np.zeros((count, OBS), np.float32)
    ang = rng.uniform(-np.pi, np.pi, count)
    rng_m = rng.uniform(0.15, 1.0, count)
    head = rng.uniform(-np.pi, np.pi, count)
    o[:, 0] = np.sin(ang + head) * rng_m
    o[:, 1] = np.cos(ang + head) * rng_m
    o[:, 2] = rng_m
    o[:, 3] = np.sin(ang)
    o[:, 4] = np.cos(ang)
    o[:, 5] = rng.uniform(-0.3, 1.0, count)
    o[:, 6] = rng.uniform(0, 1, count)
    o[:, 7] = 1.0
    o[:, 8] = rng.choice([-1.0, 1.0], count, p=[0.2, 0.8])
    o[:, 9] = 1.0
    o[:, 10] = rng.uniform(-1, 1, count)
    o[:, 11] = 1.0
    o[:, 12] = rng.uniform(-0.6, 0.6, count)
    o[:, 13] = rng.uniform(-0.6, 0.6, count)
    o[:, 14:18] = 1.0
    o[:, 18] = rng.uniform(-0.1, 0.1, count)
    o[:, 20] = o[:, 3]
    o[:, 21] = o[:, 4]
    o[:, 22] = rng_m
    o[:, 23] = 1.0
    return o


def july_mask():
    """True where a weight belongs to the July net (input<14 to unit<16, those units' biases and
    output weights, the output biases)."""
    m = np.zeros(GENOME, bool)
    for j in range(JULY_HID):
        m[j * OBS : j * OBS + JULY_OBS] = True
        m[OBS * HID + j] = True
        for k in range(ACT):
            m[OBS * HID + HID + k * HID + j] = True
    m[OBS * HID + HID + HID * ACT :] = True
    return m


# The owner's standing rule (2026-11-03): before any compute, check that his game or editor is not
# live; while it is, start nothing. Otherwise at most `--workers` (two) episodes at once. Checked
# right before every episode starts.
# The bracketed first letters keep the pattern from matching a command line that merely spells it.
OWNER_GAME = r"release/[p]anzerrechnersystem( fea-range| edit)?$|[l]aunch_demo"
_gpu = threading.Condition()
_running = 0


def owner_playing():
    return subprocess.run(["pgrep", "-f", OWNER_GAME], capture_output=True).returncode == 0


def episode(args, path, seed):
    global _running
    with _gpu:
        while owner_playing() or _running >= args.workers:
            _gpu.wait(timeout=5.0)
        _running += 1
    try:
        return _episode(args, path, seed)
    finally:
        with _gpu:
            _running -= 1
            _gpu.notify_all()


def _episode(args, path, seed):
    env = dict(os.environ, **ENV)
    cmd = ["nice", "-n", "10", args.bin, "t34-canyon-eval", str(seed), str(args.secs)]
    p = subprocess.run(cmd, input=path + "\n", capture_output=True, text=True, env=env, cwd=args.scratch)
    for line in p.stdout.splitlines():
        if line.startswith("[t34-eval] {"):
            return json.loads(line[len("[t34-eval] "):])
    raise RuntimeError(f"episode {path} seed {seed} failed: {p.returncode}\n{p.stderr[-2000:]}\n{p.stdout[-2000:]}")


def score(ep):
    e = ep["enemy_ai"]
    shots, hits = e["shots_front_side_rear"], e["hits_on_tiger_front_side_rear"]
    parts = {
        "pens": 40.0 * e["pens_on_tiger"],
        "hits": 15.0 * (hits[1] + hits[2]) + 4.0 * hits[0],
        "shots": 4.0 * (shots[1] + shots[2]) + 1.0 * shots[0],
        "contact": 0.25 * e["contact_tank_s"],
        "closing": min(ep["closing_m"], 1500.0) / 30.0,
        "flank": 0.4 * ep["flank_s"],
        "off_bow": 0.5 * ep.get("off_bow_s", 0.0),
        "exposed": -0.6 * ep["exposed_front_s"],
        "pincer": 0.05 * ep["pincer_deg"],
        "pinned": -2.0 * ep["pinned_tank_s"],
        "dither": -0.5 * max(e["stuck_tank_s"] - ep["pinned_tank_s"], 0.0),
        "lost": -10.0 * e["tanks_knocked_out"],
        "kill": 30.0 if not ep["tiger_alive"] else 0.0,
    }
    return sum(parts.values()), parts


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", required=True)
    ap.add_argument("--init", required=True)
    ap.add_argument("--july", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--gens", type=int, default=10)
    ap.add_argument("--pop", type=int, default=10)
    ap.add_argument("--mu", type=int, default=3)
    ap.add_argument("--workers", type=int, default=2)
    ap.add_argument("--secs", type=float, default=120.0)
    ap.add_argument("--seeds", default="3,11,17,41,53,64,88,101")
    ap.add_argument("--per-gen", type=int, default=3)
    ap.add_argument("--rotate", type=int, default=2)
    ap.add_argument("--sigma-july", type=float, default=0.02)
    ap.add_argument("--sigma-new", type=float, default=0.15)
    ap.add_argument("--sigma-cross", type=float, default=0.03)
    ap.add_argument("--anchor", type=float, default=150.0)
    ap.add_argument("--base-seed", type=int, default=0x7334)
    ap.add_argument("--parents", default="", help="comma list of KSL2 files to resume from (replaces --init as parents)")
    ap.add_argument("--start-gen", type=int, default=0, help="generation index to resume at (seed rotation and mutation draws)")
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)
    args.scratch = os.path.join(args.out, "cwd")
    os.makedirs(args.scratch, exist_ok=True)
    log = open(os.path.join(args.out, "es.jsonl"), "a")
    pool = [int(s) for s in args.seeds.split(",")]

    _, n0, h0, init = load(args.init)
    assert (n0, h0) == (OBS, HID), "the init must be a KSL2 net (t34-seed)"
    _, nj, hj, july = load(args.july)
    assert (nj, hj) == (JULY_OBS, JULY_HID)
    bank = open_ground_bank(np.random.default_rng(args.base_seed))
    july_out = outputs(july, bank, JULY_OBS, JULY_HID)
    mask = july_mask()
    sigma = np.where(mask, args.sigma_july, args.sigma_new).astype(np.float32)
    # New inputs into the July units: on open ground several of them are constants (room 1, the
    # memory flag), so a large step there shifts the July manner everywhere. Keep it small; the new
    # behaviour belongs in the new units.
    for j in range(JULY_HID):
        sigma[j * OBS + JULY_OBS : (j + 1) * OBS] = args.sigma_cross
    # Output weights of the new units: several sit saturated on open ground, so a step there is a
    # constant shift of the July answers. Same small step.
    for j in range(JULY_HID, HID):
        for k in range(ACT):
            sigma[OBS * HID + HID + k * HID + j] = args.sigma_cross

    def anchor(w):
        return float(np.mean((outputs(w, bank, OBS, HID) - july_out) ** 2))

    cache = {}  # (genome digest, seed) -> episode

    def digest(w):
        return hashlib.sha1(np.asarray(w, np.float32).tobytes()).hexdigest()[:16]

    def evaluate(population, seeds):
        jobs = {}
        with cf.ThreadPoolExecutor(args.workers) as ex:
            for w in population:
                d = digest(w)
                path = os.path.join(args.out, "genomes", d + ".bin")
                if not os.path.exists(path):
                    os.makedirs(os.path.dirname(path), exist_ok=True)
                    save(path, w, 0, 0.0)
                for s in seeds:
                    if (d, s) not in cache and (d, s) not in jobs:
                        jobs[(d, s)] = ex.submit(episode, args, path, s)
            for k, f in jobs.items():
                try:
                    cache[k] = f.result()
                except Exception as error:  # a crashed episode scores as a disaster, loudly
                    print(f"[es] episode {k} failed: {error}", file=sys.stderr, flush=True)
                    cache[k] = None
        results = []
        for w in population:
            d = digest(w)
            eps = [cache[(d, s)] for s in seeds]
            scored = [score(e) if e else (-1000.0, {"failed": -1000.0}) for e in eps]
            eps = [e or {"enemy_ai": {"stuck_tank_s": float("nan")}} for e in eps]
            a = anchor(w)
            fit = float(np.mean([s for s, _ in scored])) - args.anchor * a
            results.append((fit, a, d, eps, scored))
        return results

    parents = [load(p)[3].copy() for p in args.parents.split(",") if p] or [init.copy()]
    best = None
    t0 = time.time()
    for gen in range(args.start_gen, args.start_gen + args.gens):
        window = gen // args.rotate
        seeds = [pool[(window * args.per_gen + k) % len(pool)] for k in range(args.per_gen)]
        population = [p.copy() for p in parents]
        child = 0
        while len(population) < args.pop:
            rng = np.random.default_rng([args.base_seed, gen, child])
            parent = parents[child % len(parents)]
            population.append(np.clip(parent + rng.standard_normal(GENOME).astype(np.float32) * sigma, -4, 4))
            child += 1
        results = evaluate(population, seeds)
        order = sorted(range(len(population)), key=lambda i: -results[i][0])
        parents = [population[i] for i in order[: args.mu]]
        top = results[order[0]]
        for rank, i in enumerate(order):
            fit, a, d, eps, scored = results[i]
            log.write(json.dumps({"gen": gen, "rank": rank, "fitness": fit, "anchor_mse": a, "genome": d,
                "seeds": seeds, "parts": [p for _, p in scored],
                "stuck_tank_s": [e["enemy_ai"]["stuck_tank_s"] for e in eps]}) + "\n")
        log.flush()
        champ = os.path.join(args.out, f"gen_{gen:03d}_{top[2]}.bin")
        save(champ, parents[0], gen, top[0])
        print(f"[es] gen {gen} seeds {seeds}: best {top[0]:.1f} (anchor {top[1]:.4f}, stuck "
              f"{[round(e['enemy_ai']['stuck_tank_s'], 1) for e in top[3]]}) mean "
              f"{np.mean([r[0] for r in results]):.1f}; {(time.time() - t0) / 60:.1f} min -> {champ}", flush=True)
        best = champ
    print(f"[es] done: {best}")


if __name__ == "__main__":
    sys.exit(main())
