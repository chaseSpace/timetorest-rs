export function mergeWhitelist(current: string, added: string[]): string[] {
  const names: string[] = [];
  for (const entry of [...current.split(/\r?\n/), ...added]) {
    const name = entry.trim();
    if (!name) continue;
    if (new TextEncoder().encode(name).length > 80 || /[\/\\\r\n]/.test(name))
      throw new Error("请填写进程文件名，不要填写完整路径");
    if (!names.some((old) => old.toLowerCase() === name.toLowerCase()))
      names.push(name);
  }
  if (names.length > 5)
    throw new Error("白名单最多 5 个程序，请先移除部分程序");
  return names;
}
