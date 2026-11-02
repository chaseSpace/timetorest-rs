import type { Settings } from "./model";

export const MAX_UPLOAD = 5_000_000;
export const MAX_STORED = 500_000;
export const backgrounds = [
  {
    id: "forest",
    label: "林间",
    src: "/wallpapers/forest.jpg",
    credit: "Ingmar / Unsplash",
  },
  {
    id: "mountain",
    label: "远山",
    src: "/wallpapers/mountain.jpg",
    credit: "Luca Bravo / Unsplash",
  },
  {
    id: "night",
    label: "星夜",
    src: "/wallpapers/night.jpg",
    credit: "David Schaubert / Unsplash",
  },
];
export function photo(settings: Settings) {
  return (
    backgrounds.find((item) => item.id === settings.theme) || backgrounds[0]
  );
}
export function checkUpload(file: File) {
  if (file.size > MAX_UPLOAD || !file.size)
    throw new Error("请选择不超过 5 MB 的图片");
  if (!["image/jpeg", "image/png", "image/webp"].includes(file.type))
    throw new Error("仅支持 JPEG、PNG 或 WebP 图片");
}
export function base64(file: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result).split(",")[1]);
    reader.onerror = () => reject(new Error("读取图片失败"));
    reader.readAsDataURL(file);
  });
}

// Browser preview only. Native uploads are decoded and bounded independently in Rust.
export async function preparePreview(data: string) {
  if (data.length > Math.ceil(MAX_UPLOAD / 3) * 4)
    throw new Error("请选择不超过 5 MB 的图片");
  const source = Uint8Array.from(atob(data), (c) => c.charCodeAt(0));
  if (!source.length || source.length > MAX_UPLOAD)
    throw new Error("请选择不超过 5 MB 的图片");
  const png =
    source[0] === 137 &&
    source[1] === 80 &&
    source[2] === 78 &&
    source[3] === 71;
  const jpeg = source[0] === 255 && source[1] === 216;
  const webp =
    new TextDecoder().decode(source.slice(0, 4)) === "RIFF" &&
    new TextDecoder().decode(source.slice(8, 12)) === "WEBP";
  if (!png && !jpeg && !webp) throw new Error("仅支持 JPEG、PNG 或 WebP 图片");
  const image = await createImageBitmap(new Blob([source]));
  try {
    if (
      image.width > 8192 ||
      image.height > 8192 ||
      image.width * image.height > 16_000_000
    )
      throw new Error("图片最多 1600 万像素，任一边不能超过 8192 像素");
    const scale = Math.min(1, 1920 / image.width, 1200 / image.height);
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(image.width * scale));
    canvas.height = Math.max(1, Math.round(image.height * scale));
    const ctx = canvas.getContext("2d")!;
    ctx.fillStyle = "#121212";
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(image, 0, 0, canvas.width, canvas.height);
    for (const quality of [0.85, 0.75, 0.65, 0.55, 0.45, 0.3, 0.15]) {
      const blob = await new Promise<Blob>((resolve, reject) =>
        canvas.toBlob(
          (blob) => (blob ? resolve(blob) : reject(new Error("图片压缩失败"))),
          "image/jpeg",
          quality,
        ),
      );
      if (blob.size <= MAX_STORED) {
        const bytes = await blob.arrayBuffer();
        const hash = [
          ...new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)),
        ]
          .map((b) => b.toString(16).padStart(2, "0"))
          .join("");
        return {
          name: `custom-${hash}.jpg`,
          dataUrl: `data:image/jpeg;base64,${await base64(blob)}`,
        };
      }
    }
    throw new Error("图片无法压缩到 500 KB 以内，请换一张图片");
  } finally {
    image.close();
  }
}
