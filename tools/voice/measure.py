"""Measurements for crew voices v2: f0 (pYIN) median/p95/max-sustained, share of energy below 300 Hz,
and an ASR check (Whisper large-v3-turbo, local). Library + CLI.
venv-qwen/bin/python -I measure.py ROLE_TEXT_JSON out.json  (list of {file, text})"""
import sys, json, re, unicodedata
import numpy as np, soundfile as sf, librosa

def load(path, sr=22050):
    a, r = sf.read(path, dtype="float32", always_2d=True)
    a = a.mean(axis=1)
    return (librosa.resample(a, orig_sr=r, target_sr=sr) if r != sr else a), sr

def active_mask(y, hop=220, floor_db=-35):
    rms = librosa.feature.rms(y=y, frame_length=882, hop_length=hop)[0]
    return rms > (rms.max() * 10 ** (floor_db / 20)), rms

def f0_stats(y, sr=22050):
    hop = 220
    f0, vflag, vprob = librosa.pyin(y, fmin=50, fmax=500, sr=sr, frame_length=1764, hop_length=hop)
    ok = vflag & ~np.isnan(f0)  # pYIN voiced flag (a rough, gravelly voice has low voicing probabilities)
    # Only frames with speech energy (within 35 dB of the loudest): a faint tone in a silent tail
    # (Chatterbox leaves one near 6 kHz) otherwise reads as 'voiced' at a few hundred Hz.
    act, _ = active_mask(y)
    n = min(len(ok), len(act)); ok = ok[:n] & act[:n]; f0 = f0[:n]
    f = f0[ok]
    if len(f) < 5:
        return dict(f0_median=float("nan"), f0_p5=float("nan"), f0_p95=float("nan"), f0_max_sustained=float("nan"),
                    frac_above_200=float("nan"), voiced_frames=int(len(f)))
    # highest pitch held for >= 50 ms (5 consecutive voiced frames), so one-frame tracker errors don't count
    sustained = 0.0
    run = []
    for v, k in zip(f0, ok):
        if k:
            run.append(v)
            if len(run) >= 5:
                sustained = max(sustained, float(np.min(run[-5:])))
        else:
            run = []
    return dict(f0_median=float(np.median(f)), f0_p5=float(np.percentile(f, 5)), f0_p95=float(np.percentile(f, 95)),
                f0_max_sustained=sustained, frac_above_200=float(np.mean(f > 200)), voiced_frames=int(len(f)))

def band_shares(y, sr=22050):
    """Energy shares over active frames, within 60 Hz-8 kHz: <300 Hz (body 120-300 too), 300-3k, >3k."""
    act, _ = active_mask(y)
    S = np.abs(librosa.stft(y, n_fft=2048, hop_length=220)) ** 2
    n = min(S.shape[1], len(act)); S = S[:, :n][:, act[:n]]
    fr = librosa.fft_frequencies(sr=sr, n_fft=2048)
    band = (fr >= 60) & (fr <= 8000)
    tot = S[band].sum() + 1e-12
    share = lambda lo, hi: float(S[(fr >= lo) & (fr < hi)].sum() / tot)
    return dict(share_lt300=share(60, 300), share_120_300=share(120, 300), share_300_3k=share(300, 3000),
                share_gt3k=share(3000, 8000.1))

_ONES = ["", "ein", "zwo", "drei", "vier", "fünf", "sechs", "sieben", "acht", "neun", "zehn", "elf", "zwölf"]


def _say(n):
    """Digits the recogniser writes, in the spoken form of the lines ("7" sieben, "200" zwohundert)."""
    if n <= 12:
        return _ONES[n]
    if n % 50 == 0 and 100 <= n <= 1950:
        h, rest = divmod(n, 100)
        head = ("" if h == 1 else _ONES[h]) + "hundert"
        return head + ("fünfzig" if rest else "")
    return str(n)


def norm(s):
    s = unicodedata.normalize("NFC", s.lower())
    s = re.sub(r"(?<=\d)\.(?=\d{3})", "", s)  # 1.200 -> 1200
    s = re.sub(r"\d+", lambda m: " " + _say(int(m.group())) + " ", s)
    s = re.sub(r"[^a-zäöüß0-9 ]+", " ", s)
    s = s.replace("noch mal", "nochmal")
    num = {"2": "zwo", "zwei": "zwo", "3": "drei", "800": "achthundert", "100": "hundert", "1": "ein", "eins": "ein",
           "einen": "ein", "einem": "ein"}
    return " ".join(num.get(w, w) for w in s.split())

def edit(a, b):
    d = list(range(len(b) + 1))
    for i in range(1, len(a) + 1):
        p, d[0] = d[0], i
        for j in range(1, len(b) + 1):
            p, d[j] = d[j], min(d[j] + 1, d[j - 1] + 1, p + (a[i - 1] != b[j - 1]))
    return d[len(b)]

_asr = None
def asr(path, device="cuda"):
    global _asr
    if _asr is None:
        import torch
        from transformers import pipeline
        _asr = pipeline("automatic-speech-recognition", model="openai/whisper-large-v3-turbo",
                        device=0 if device == "cuda" else -1,
                        torch_dtype=torch.float16 if device == "cuda" else torch.float32)
    y, _ = load(path, 16000)
    return _asr({"raw": y, "sampling_rate": 16000}, generate_kwargs={"language": "german", "task": "transcribe"})["text"].strip()

def measure(path, text=None, do_asr=True, device="cuda"):
    y, sr = load(path)
    act, _ = active_mask(y)
    r = dict(file=str(path), dur=len(y) / sr, active_s=float(act.sum() * 220 / sr),
             peak_db=float(20 * np.log10(np.abs(y).max() + 1e-12)))
    r.update(f0_stats(y, sr)); r.update(band_shares(y, sr))
    if do_asr and text:
        hyp = asr(path, device)
        rn, hn = norm(text), norm(hyp)
        r.update(asr=hyp, cer=edit(rn, hn) / max(1, len(rn)), wer=edit(rn.split(), hn.split()) / max(1, len(rn.split())))
    return r

if __name__ == "__main__":
    items = json.load(open(sys.argv[1]))
    dev = sys.argv[3] if len(sys.argv) > 3 else "cuda"
    out = [measure(it["file"], it.get("text"), do_asr=bool(it.get("text")), device=dev) | {k: v for k, v in it.items() if k not in ("file",)} for it in items]
    json.dump(out, open(sys.argv[2], "w"), indent=1, ensure_ascii=False)
    for r in out:
        print(f"{r['file'].split('/')[-1]:50s} f0 med {r['f0_median']:6.1f} p95 {r['f0_p95']:6.1f} sus {r['f0_max_sustained']:6.1f} "
              f"<300 {r['share_lt300']:.2f} cer {r.get('cer', float('nan')):.2f} | {r.get('asr', '')}")
