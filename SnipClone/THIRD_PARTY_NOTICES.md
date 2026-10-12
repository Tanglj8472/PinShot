# Third-party notices

## PowerToys / ZoomIt (MIT)

SnipClone is a **clean redesign** that references public APIs and interaction
patterns from Microsoft PowerToys ZoomIt (`src/modules/ZoomIt/`), especially:

- DPI / monitor helpers (`Utility.cpp` concepts)
- Rectangle selection drag / ESC cancel (`SelectRectangle`)
- Layered topmost overlay window style
- Per-Monitor DPI awareness manifest settings

This repository does **not** vendor the ZoomIt sources wholesale. Logic was
rewritten for Snipaste-style “select region first, then capture”.

```
The MIT License

Copyright (c) Microsoft Corporation. All rights reserved.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

ZoomIt itself also carries Sysinternals / www.sysinternals.com authorship in
upstream file headers. Preserve those notices if you later extract or adapt
additional ZoomIt files directly.

Upstream: https://github.com/microsoft/PowerToys  
License file: https://github.com/microsoft/PowerToys/blob/main/LICENSE
