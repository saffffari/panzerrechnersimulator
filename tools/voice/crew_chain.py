"""Crew intercom / radio chains, v2 (2026-11-08), and the v1 intercom chain kept for comparison.

v2 intercom: throat microphone -> intercom amplifier -> headset. Keeps the chest body (only a gentle
85 Hz high-pass), rolls the top off from ~4 kHz (a throat mic picks the voice up through the neck,
so sibilance and air are weak), a low-mid lift, two compressors around a tanh saturation stage
(the amplifier driven hard), and a light bed: hiss plus the Maybach's firing tone leaking in.
v2 radio (net): the same, a little narrower (140 Hz - 3.4 kHz), driven harder, carrier hiss while
keyed and a key click; AM sets of the period had no squelch tail.
No pitch or tempo processing anywhere: the voice's own pitch is what is heard.
"""
import subprocess
import numpy as np

SR = 48000

IC2_VOICE = (
    "aresample=48000:resampler=soxr,aformat=sample_fmts=fltp:channel_layouts=mono,"
    "adelay=delays=20:all=1,apad=pad_dur=0.15,"
    "highpass=f=85:poles=2,"
    "lowpass=f=4200:poles=2,lowpass=f=4200:poles=2,"
    "equalizer=f=170:t=o:w=1.3:g=3,equalizer=f=650:t=o:w=1.0:g=2.5,equalizer=f=2700:t=o:w=1.0:g=-3,"
    "acompressor=threshold=0.1:ratio=4:attack=4:release=120:knee=4:makeup=2,"
    "volume=9dB,asoftclip=type=tanh:oversample=4,volume=-5dB,"
    "acompressor=threshold=0.22:ratio=8:attack=1:release=60:knee=2:makeup=1.6,"
    "highpass=f=70:poles=2,lowpass=f=5000:poles=2"
)
RADIO2_VOICE = (
    "aresample=48000:resampler=soxr,aformat=sample_fmts=fltp:channel_layouts=mono,"
    "adelay=delays=60:all=1,apad=pad_dur=0.12,"
    "highpass=f=140:poles=2,highpass=f=140:poles=2,"
    "lowpass=f=3400:poles=2,lowpass=f=3400:poles=2,"
    "equalizer=f=220:t=o:w=1.2:g=2,equalizer=f=1200:t=o:w=1.0:g=3,"
    "acompressor=threshold=0.08:ratio=6:attack=3:release=90:knee=3:makeup=2.5,"
    "volume=12dB,asoftclip=type=tanh:oversample=4,volume=-6dB,"
    "acompressor=threshold=0.2:ratio=10:attack=1:release=50:makeup=1.5,"
    "highpass=f=120:poles=2,lowpass=f=3800:poles=2"
)

# Beds, generated to length T: hiss (pinkish) + engine firing tone (~200 Hz at 2,000 rpm on a V12
# four-stroke: 2000/60*6) and its harmonic, slowly wobbling with the load.
def ic_bed(T):
    return (f"anoisesrc=d={T:.4f}:c=pink:r={SR}:a=0.010:seed=7,highpass=f=120,lowpass=f=3800[hiss];"
            f"aevalsrc=exprs='0.0035*(sin(2*PI*200*t)+0.5*sin(2*PI*400*t+1))*(1+0.25*sin(2*PI*2.7*t))':s={SR}:d={T:.4f}:c=mono[eng];"
            f"[hiss][eng]amix=inputs=2:normalize=0")

def radio_bed(T, key_off):
    return (f"anoisesrc=d={T:.4f}:c=white:r={SR}:a=0.03:seed=11,highpass=f=300,lowpass=f=3200,"
            f"volume='if(lt(t,{key_off:.4f}),1,0)':eval=frame[hiss];"
            f"aevalsrc=exprs='if(lt(t,0.006),0.5*exp(-t*650)*(1-2*gte(mod(t,0.0008),0.0004)),0)"
            f"+if(between(t,{key_off:.4f},{key_off:.4f}+0.006),0.45*exp(-(t-{key_off:.4f})*650)*(1-2*gte(mod(t-{key_off:.4f},0.0007),0.00035)),0)'"
            f":s={SR}:d={T:.4f}:c=mono,highpass=f=150,lowpass=f=3500[clk];"
            f"[hiss][clk]amix=inputs=2:normalize=0")

LIMIT = "alimiter=limit=0.89:attack=1:release=50:level=0"

# ------------------------------------------------------------- v1 intercom (tts-radio-20261108)
IC1_VOICE = (
    "aresample=44100:resampler=soxr,aformat=sample_fmts=fltp:channel_layouts=mono,"
    "adelay=delays=25:all=1,apad=whole_dur={T},"
    "highpass=f=420:poles=2,highpass=f=420:poles=2,lowpass=f=2200:poles=2,lowpass=f=2200:poles=2,"
    "equalizer=f=1050:t=o:w=0.7:g=8,equalizer=f=1800:t=o:w=0.5:g=-4,"
    "acompressor=threshold=0.06:ratio=5:attack=5:release=150:knee=3:makeup=2.5,"
    "volume=3dB,asoftclip=type=tanh"
)
IC1_BED = ("aevalsrc=exprs='0.02*(random(0)*2-1)+0.004*(sin(2*PI*250*t)+0.6*sin(2*PI*500*t+1)"
           "+0.4*sin(2*PI*750*t+2))*(1+0.3*sin(2*PI*3.1*t))':s=44100:d={T}:c=mono,"
           "highpass=f=420:poles=2,lowpass=f=2200:poles=2")
IC1_RX = ("highpass=f=420:poles=2,highpass=f=420:poles=2,highpass=f=420:poles=2,"
          "lowpass=f=2200:poles=2,lowpass=f=2200:poles=2,lowpass=f=2200:poles=2")


def ff(args):
    cmd = ["ffmpeg", "-v", "error", "-nostdin", "-threads", "2"] + args + ["-ac", "1", "-ar", str(SR), "-f", "f32le", "-"]
    return np.frombuffer(subprocess.run(cmd, capture_output=True, check=True).stdout, dtype="<f4").copy()

def duration(path):
    r = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", str(path)],
                       capture_output=True, text=True, check=True)
    return float(r.stdout.strip())

def intercom_v2(path):
    T = duration(path) + 0.02 + 0.15
    g = f"[0:a]{IC2_VOICE}[v];{ic_bed(T)}[b];[v][b]amix=inputs=2:normalize=0:duration=first,{LIMIT}"
    return ff(["-i", str(path), "-filter_complex", g])

def radio_v2(path):
    D = duration(path)
    T = D + 0.06 + 0.12
    g = f"[0:a]{RADIO2_VOICE}[v];{radio_bed(T, D + 0.06 + 0.06)}[b];[v][b]amix=inputs=2:normalize=0:duration=first,{LIMIT}"
    return ff(["-i", str(path), "-filter_complex", g])

def intercom_v1(path):
    T = 0.025 + duration(path) + 0.12
    f = dict(T=f"{T:.4f}")
    g = (f"[0:a]{IC1_VOICE.format(**f)}[v];{IC1_BED.format(**f)}[b];"
         f"[v][b]amix=inputs=2:normalize=0:duration=first,{IC1_RX},volume=4dB,{LIMIT}")
    return ff(["-i", str(path), "-filter_complex", g])

def write_wav(path, a, sr=SR):
    import wave
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(sr)
        w.writeframes((np.clip(a, -1, 1) * 32767).round().astype("<i2").tobytes())

def dry(path, peak_db=-3.0):
    a = ff(["-i", str(path), "-af", "aresample=48000:resampler=soxr"])
    return a * (10 ** (peak_db / 20) / (np.abs(a).max() + 1e-9))
