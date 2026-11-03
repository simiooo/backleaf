# 可见范围与流光调度模型

```powershell
rustc --edition 2024 -O spikes/viewport-scale/main.rs -o .local/viewport-scale.exe
.local/viewport-scale.exe
```

只依赖 Rust 标准库。模型检查可见区/预取边界、深处相对坐标、有界短名称缓存、复用后的 tooltip 身份、单共享时钟与减少动态效果。它按需生成可见文件的短名称，不构建百万条目的数组，也不打开、改动或删除用户文件。

这不是 GPUI 应用，不包含真实绘制或布局测量。范围计算的计时不能推导帧率；实际 GPU、字体、省略、可访问性与多窗口还需独立集成验证。当前实测与源码哈希保存在 [证据](../../docs/evidence/viewport-scale-model.json)。
