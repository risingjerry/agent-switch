Regenerate `docs/social-preview.jpg`: `background.jpg` is a text-free oil
painting made with an image model; `icon-glyph.png` is the app icon's glyph.

```bash
python3 social_preview.py --background docs/social-preview/background.jpg \
  --icon docs/social-preview/icon-glyph.png --name "Fusebox|MCP & Skills" \
  --veil 0.15 --out docs/social-preview.jpg
```

`social_preview.py` comes from the open-source-launch skill.
