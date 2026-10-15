/** `#rgb` / `#rgba` → `#rrggbb` / `#rrggbbaa`: Monaco rejects the short forms, and the CSS
 * minifier of release builds shortens e.g. `#FFFFFF` to `#fff`. */
export function expandHex(color: string): string {
  const short = /^#([0-9a-f]{3,4})$/i.exec(color);
  if (!short) return color;
  return `#${[...short[1]!].map((c) => c + c).join("")}`;
}
