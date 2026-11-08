#!/usr/bin/env python3
"""Preserve native picture frames; extend/pitch the recorded impact's low band for slow inspection.
Never overwrites the native source. This is a cinematic mix, not acoustic propagation validation.
"""
import argparse,json,subprocess
from pathlib import Path

def run(report_path):
 report_path=Path(report_path);report=json.loads(report_path.read_text());fps=report['fps'];done=[]
 for c in report['clips']:
  if not c.get('complete') or c.get('real_time'):continue
  source=Path(c['video']);dest=source.with_stem(source.stem+'-deep')
  onset=c['impact_frame']/fps
  # End the sustained pressure layer with the smooth return to real time.
  last=c['segments'][-1]['start_frame']/fps
  duration=max(2.,last-onset);seed=min(.9,duration)
  fade=min(4.,duration*.25)
  graph=(f'[0:a]asplit=2[dry][seed];'
    f'[dry]volume=\'if(between(t,{onset:.8f},{last:.8f}),0.6,1)\':eval=frame[base];'
    f'[seed]atrim=start={onset:.8f}:duration={seed},asetpts=PTS-STARTPTS,'
    f'highpass=f=26,lowpass=f=450,rubberband=tempo={seed/duration:.8f}:pitch=0.42,'
    f'apad,atrim=duration={duration:.8f},afade=t=in:d=0.02,'
    f'afade=t=out:st={duration-fade:.8f}:d={fade:.8f},volume=0.85,'
    f'adelay={round(onset*1000)}|{round(onset*1000)}[pressure];'
    f'[base][pressure]amix=inputs=2:duration=first:normalize=0,alimiter=limit=0.89:level=0[out]')
  if dest.exists():raise FileExistsError(dest)
  subprocess.run(['ffmpeg','-v','error','-nostdin','-i',str(source),'-filter_complex',graph,'-map','0:v:0','-map','[out]','-c:v','copy','-c:a','aac','-b:a','192k','-movflags','+faststart',str(dest)],check=True)
  subprocess.run(['ffmpeg','-v','error','-i',str(dest),'-f','null','-'],check=True)
  probe=json.loads(subprocess.check_output(['ffprobe','-v','error','-show_streams','-show_format','-of','json',str(dest)]))
  c['cinematic_audio_video']=str(dest)
  done.append(dict(source=str(source),output=str(dest),impact_seconds=onset,pressure_duration_seconds=duration,pitch_ratio=.42,pressure_gain=.85,low_band_hz=[26,450],native_detail_gain=.6,video_copy=True,probe=probe))
  print(dest.name,'verified',flush=True)
 out=report_path.with_name('audio-finish.json');out.write_text(json.dumps({'clips':done,'note':'Cinematic low-band extension; original event-driven ricochets and subtle whizzes retained. Native source videos unchanged.'},indent=2))
 report_path.with_name('finished-report.json').write_text(json.dumps(report,indent=2))
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('report');a=p.parse_args();run(a.report)
