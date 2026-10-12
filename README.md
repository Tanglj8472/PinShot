# SnipClone

独立分支：本仓库在此分支**仅保留**原生 Win32 / Direct2D 截图批注项目。

Python PinShot 与其它产物不在本分支；见 `main`。

完整说明、构建与发布见 [`SnipClone/README.md`](SnipClone/README.md)。

```powershell
cd SnipClone
.\build.ps1 -Configuration Release -Platform x64
```

CI：`.github/workflows/build-snipclone.yml`（路径过滤 `SnipClone/**`）。
