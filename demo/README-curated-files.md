# Curated demo files

Drop real files into `demo/files/` to make them the cloud's content for **every**
demo visitor (read-only — visitors can browse, download, and "buy" them, but only
maintainers add them). This is the "private login": the only way to add files is
to commit them here, which requires repo access.

```
demo/files/
  Documents/whitepaper.pdf
  Photos/rig.jpg
  Music/track.mp3
```

- Folder structure becomes the cloud's folders.
- Text files (`.md .txt .json .csv …`, < 1 MB) are inlined; everything else is
  streamed from disk on download.
- If `demo/files/` is empty, the demo falls back to the built-in seeded set
  (Documents/Photos/Music/Videos with sample content).

After adding files, commit and push — CI rebuilds the `:demo` image and Portainer
redeploys. Keep the total modest (these load into the demo image).
