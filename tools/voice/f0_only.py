"""f0 and band shares only (no ASR), CPU. python -I f0_only.py FILE..."""
import os, sys, json
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # tools/voice (a copy of the work dir's scripts)
sys.path.insert(0, "/home/alex/panzer/tts-radio-work-20261108/crew-v2/scripts")
from measure import measure
for f in sys.argv[1:]:
    r = measure(f, do_asr=False)
    print(f"{f.split('/')[-1]:28s} med {r['f0_median']:6.1f} p5 {r['f0_p5']:6.1f} p95 {r['f0_p95']:6.1f} sus {r['f0_max_sustained']:6.1f} >200 {r['frac_above_200']:.2f} <300 {r['share_lt300']:.2f} 120-300 {r['share_120_300']:.2f} dur {r['dur']:.1f}")
