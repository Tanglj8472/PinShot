# Capture（V1）

区域屏幕捕获：Windows.Graphics.Capture（优先）+ GDI BitBlt fallback。

输入：`SelectionOverlay::LastSelection().screenRect`  
输出：BGRA 位图 → 剪贴板 / 批注底图。
