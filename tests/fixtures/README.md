# Synthetic Opus regression fixtures

These files contain generated 440 Hz tones, not recordings or user audio.
FFmpeg is needed only to regenerate them, not to run the tests or application.

`opus-mono.ogg` contains 48,013 mono frames at 48 kHz. `opus-stereo.ogg`
contains 96,013 stereo frames at 48 kHz with opposite channels, so downmixing
should cancel the tone. Both end partway through an Opus packet to check
encoder delay and final padding removal. `opus-short.ogg` contains 161 frames
at an original input rate of 16 kHz (483 decoded frames at Opus's 48 kHz),
covering a clip shorter than one packet and resampler block.

Generated with FFmpeg 9.0 / libopus:

```sh
ffmpeg -f lavfi -i 'aevalsrc=0.3*sin(2*PI*440*t):s=48000' -af 'atrim=end_sample=48013' -c:a libopus -application voip -b:a 24k opus-mono.ogg
ffmpeg -f lavfi -i 'aevalsrc=0.3*sin(2*PI*440*t)|-0.3*sin(2*PI*440*t):s=48000' -af 'atrim=end_sample=96013' -c:a libopus -b:a 64k opus-stereo.ogg
ffmpeg -f lavfi -i 'aevalsrc=0.3*sin(2*PI*440*t):s=16000' -af 'atrim=end_sample=161' -c:a libopus -application voip -b:a 24k opus-short.ogg
```
