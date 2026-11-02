# 默认休息背景图片

2026-10-31 从 Unsplash 取得，图片遵循 [Unsplash License](https://unsplash.com/license)，独立于应用代码许可。图片在构建时打包，程序运行时只读取本地文件。

| 文件 | 摄影师与原始页面 | 下载链接 | 字节数 |
| --- | --- | --- | --- |
| forest.jpg | [Ingmar](https://unsplash.com/photos/the-sun-is-shining-through-the-trees-in-the-forest-I1j5TkSutxk) | [1600×1000 JPEG，质量 35](https://images.unsplash.com/photo-1739361133019-b995c04044bc?fm=jpg&fit=crop&w=1600&h=1000&q=35) | 209414 |
| mountain.jpg | [Luca Bravo](https://unsplash.com/photos/brown-house-near-body-of-water-zAjdgNXsMeg) | [1600×1000 JPEG，质量 60](https://images.unsplash.com/photo-1470770841072-f978cf4d019e?fm=jpg&fit=crop&w=1600&h=1000&q=60) | 286429 |
| night.jpg | [David Schaubert](https://unsplash.com/photos/a-night-sky-with-stars-above-a-mountain-range-jzokxnaObw8) | [1600×1000 JPEG，质量 60](https://images.unsplash.com/photo-1652610797951-c9992b49496a?fm=jpg&fit=crop&w=1600&h=1000&q=60) | 246028 |

每张最多 300000 字节，合计 741871 字节。界面中保留摄影师署名。所有默认图片均为普通免费图片，无人物、商标或文字叠印。

自定义图片：仅 JPEG / PNG / WebP；原文件最多 5000000 字节、1600 万像素、任一边最多 8192 像素。Rust 独立校验，校正方向，只缩小至 1920×1200 内，去除原始元数据并转为最多 500000 字节的 JPEG。只保存一张当前自定义图片；替换后清理旧文件。背景数据不进入每秒计时事件，页面仅在背景变化时加载。
