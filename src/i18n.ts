import messages from "../locales/en.json";

export type Language = "zh-CN" | "en";
const english: Record<string, string> = messages;
let language: Language = "zh-CN";
export const getLocale = () => language;
export function setLocale(value: Language) {
  language = value === "en" ? "en" : "zh-CN";
  document.documentElement.lang = language;
  document.title = tr("一叶 · LeafShield");
}
const reverse = Object.fromEntries(
  Object.entries(english).map(([zh, en]) => [en, zh]),
);
const escape = (value: string) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
function patterns(catalog: Record<string, string>) {
  return Object.entries(catalog)
    .filter(([key]) => /\{\w+\}/.test(key))
    .sort(([a], [b]) => b.length - a.length)
    .map(([key, target]) => {
      const names: string[] = [];
      const expression = key
        .split(/(\{\w+\})/)
        .map((part) => {
          if (/^\{\w+\}$/.test(part)) {
            names.push(part.slice(1, -1));
            return "(.*?)";
          }
          return escape(part);
        })
        .join("");
      return { expression: new RegExp(`^${expression}$`, "s"), names, target };
    });
}
const forwardPatterns = patterns(english);
const reversePatterns = patterns(reverse);
export function tr(value: string | number | null | undefined): string {
  const source = String(value ?? "");
  const text = source.trim();
  const catalog = language === "en" ? english : reverse;
  let result = Object.hasOwn(catalog, text) ? catalog[text] : undefined;
  if (result === undefined) {
    for (const pattern of language === "en"
      ? forwardPatterns
      : reversePatterns) {
      const match = pattern.expression.exec(text);
      if (match) {
        result = pattern.target.replace(
          /\{(\w+)\}/g,
          (_, name: string) => match[pattern.names.indexOf(name) + 1],
        );
        break;
      }
    }
  }
  if (result === undefined) {
    const parts = /^(.*?)(?:：|:)\s*(.*)$/s.exec(text);
    if (parts) {
      const left = tr(parts[1]);
      const right = tr(parts[2]);
      if (left !== parts[1] || right !== parts[2]) result = `${left}: ${right}`;
    }
  }
  return result === undefined ? source : source.replace(text, () => result!);
}
export function localizeTree(root: ParentNode) {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  while (walker.nextNode()) {
    const node = walker.currentNode as Text;
    if (!node.parentElement?.closest("code, pre, textarea, [data-no-i18n]"))
      node.data = tr(node.data);
  }
  root
    .querySelectorAll<HTMLElement>("[aria-label], [title], [placeholder]")
    .forEach((element) => {
      for (const attribute of ["aria-label", "title", "placeholder"])
        if (element.hasAttribute(attribute))
          element.setAttribute(attribute, tr(element.getAttribute(attribute)));
    });
}
export function html(markup: string): string {
  const template = document.createElement("template");
  template.innerHTML = markup;
  localizeTree(template.content);
  return template.innerHTML;
}
export const zhPhrases = [
  "我知道休息不足可能会伤害眼睛和大脑，继续解锁",
  "工作可以稍等片刻，我的眼睛值得被认真照顾",
  "我选择有意识地继续工作，并记得稍后好好休息",
  "停下来不是浪费时间，而是为了更清醒地出发",
  "此刻我愿意放慢脚步，给眼睛和身体一点温柔",
];
export const phrasesFor = (locale: Language) =>
  zhPhrases.map((phrase) => (locale === "en" ? english[phrase] : phrase));
