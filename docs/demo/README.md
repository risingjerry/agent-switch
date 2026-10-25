# README demo GIF

`docs/demo.gif` is rendered from synthetic terminal frames; every name, secret, and path is fake. Commands and output lines were captured from the real CLI in an isolated `--home`/`--user-home` sandbox, then abbreviated (`...`, `~`).

## Rebuild

Requires macOS Google Chrome, `ffmpeg`, and optionally `gifsicle`. (`vhs` is not used.)

```bash
python3 docs/demo/build_demo.py
```

Edit the scenes in `build_demo.py`; `engine.py` renders HTML frames, screenshots them with headless Chrome at 2x, and assembles the GIF with ffmpeg `palettegen`/`paletteuse`. Temporary frames are deleted afterwards.
