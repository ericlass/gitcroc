// @vitest-environment node
// UI-23: colors exist only as variables in src/theme/tokens.css. This scans
// every stylesheet and every inline style in the UI for color literals.
import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test } from "vitest";

const UI = fileURLToPath(new URL("../..", import.meta.url));
const TOKENS = "src/theme/tokens.css";

// CSS Color 4 named colors. `transparent` and `currentcolor` carry no hue and
// are allowed.
const NAMED = new Set(
  (
    "aliceblue antiquewhite aqua aquamarine azure beige bisque black blanchedalmond blue blueviolet brown " +
    "burlywood cadetblue chartreuse chocolate coral cornflowerblue cornsilk crimson cyan darkblue darkcyan " +
    "darkgoldenrod darkgray darkgreen darkgrey darkkhaki darkmagenta darkolivegreen darkorange darkorchid " +
    "darkred darksalmon darkseagreen darkslateblue darkslategray darkslategrey darkturquoise darkviolet " +
    "deeppink deepskyblue dimgray dimgrey dodgerblue firebrick floralwhite forestgreen fuchsia gainsboro " +
    "ghostwhite gold goldenrod gray green greenyellow grey honeydew hotpink indianred indigo ivory khaki " +
    "lavender lavenderblush lawngreen lemonchiffon lightblue lightcoral lightcyan lightgoldenrodyellow " +
    "lightgray lightgreen lightgrey lightpink lightsalmon lightseagreen lightskyblue lightslategray " +
    "lightslategrey lightsteelblue lightyellow lime limegreen linen magenta maroon mediumaquamarine " +
    "mediumblue mediumorchid mediumpurple mediumseagreen mediumslateblue mediumspringgreen " +
    "mediumturquoise mediumvioletred midnightblue mintcream mistyrose moccasin navajowhite navy oldlace " +
    "olive olivedrab orange orangered orchid palegoldenrod palegreen paleturquoise palevioletred " +
    "papayawhip peachpuff peru pink plum powderblue purple rebeccapurple red rosybrown royalblue " +
    "saddlebrown salmon sandybrown seagreen seashell sienna silver skyblue slateblue slategray slategrey " +
    "snow springgreen steelblue tan teal thistle tomato turquoise violet wheat white whitesmoke yellow " +
    "yellowgreen"
  ).split(" "),
);

const HEX = /#[0-9a-f]{3,8}\b/gi;
const FUNCTION = /\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch|color)\(/gi;

/** The color literals in a CSS value or an inline style expression. */
function colorsIn(value: string): string[] {
  const found = [...value.matchAll(HEX), ...value.matchAll(FUNCTION)].map((m) => m[0]);
  for (const word of value.match(/[a-z]+/gi) ?? []) {
    if (NAMED.has(word.toLowerCase())) {
      found.push(word);
    }
  }
  return found;
}

/** Color literals in the declarations of a stylesheet; selectors are skipped. */
export function stylesheetColors(css: string): string[] {
  const code = css.replace(/\/\*[\s\S]*?\*\//g, "");
  return [...code.matchAll(/[-\w]+\s*:\s*([^;{}]+)[;}]/g)].flatMap((m) => colorsIn(m[1] ?? ""));
}

/** Color literals in `style={...}` and `style="..."` attributes. */
export function inlineStyleColors(source: string): string[] {
  const styles = [
    ...source.matchAll(/\bstyle=\{\{([\s\S]*?)\}\}/g),
    ...source.matchAll(/\bstyle=\{([^{][^}]*)\}/g),
    ...source.matchAll(/\bstyle="([^"]*)"/g),
    ...source.matchAll(/\bstyle='([^']*)'/g),
    ...source.matchAll(/\.style\.([\w-]+\s*=[^;\n]+)/g),
    ...source.matchAll(/\.setProperty\(([^)]*)\)/g),
  ];
  return styles.flatMap((m) => {
    // Only string contents: keys and names such as `color` are not colors.
    const strings = (m[1] ?? "").match(/"[^"]*"|'[^']*'|`[^`]*`/g) ?? [];
    return colorsIn(m[0].startsWith("style=\"") || m[0].startsWith("style='") ? (m[1] ?? "") : strings.join(" "));
  });
}

function files(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    return entry.isDirectory() ? files(path) : [path];
  });
}

test("UI-23 no hardcoded colors", () => {
  const offenders: string[] = [];
  const sources = [...files(join(UI, "src")), join(UI, "index.html")];
  for (const path of sources) {
    const name = relative(UI, path).replaceAll("\\", "/");
    if (name === TOKENS || /\.test\.tsx?$/.test(name) || name.startsWith("src/test/")) {
      continue;
    }
    const text = readFileSync(path, "utf8");
    if (name.endsWith(".css")) {
      offenders.push(...stylesheetColors(text).map((c) => `${name}: ${c}`));
    }
    if (/\.(tsx?|html)$/.test(name)) {
      offenders.push(...inlineStyleColors(text).map((c) => `${name}: ${c}`));
    }
    if (name.endsWith(".html")) {
      for (const block of text.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)) {
        offenders.push(...stylesheetColors(block[1] ?? "").map((c) => `${name}: ${c}`));
      }
    }
  }
  expect(offenders, "use a variable from src/theme/tokens.css").toEqual([]);
  // The tokens file itself must define the colors it promises.
  expect(stylesheetColors(readFileSync(join(UI, TOKENS), "utf8")).length).toBeGreaterThan(0);
});

test("UI-23 the color scan catches every kind of literal", () => {
  expect(stylesheetColors("a { color: #fff; }")).toEqual(["#fff"]);
  expect(stylesheetColors(".x { background: rgb(0 0 0 / 50%) }")).toEqual(["rgb("]);
  expect(stylesheetColors(".x { border: 1px solid hsl(0, 0%, 0%); }")).toEqual(["hsl("]);
  expect(stylesheetColors(".x { color: Red; }")).toEqual(["Red"]);
  expect(stylesheetColors(".x { color: var(--x, #000000); }")).toEqual(["#000000"]);
  expect(stylesheetColors("@media (width > 1px) { .x { outline-color: oklch(0.5 0.1 200); } }")).toEqual(["oklch("]);

  expect(stylesheetColors("#add, a:hover { color: var(--accent); border: none; }")).toEqual([]);
  expect(stylesheetColors("/* color: red */ .x { color: transparent; fill: currentcolor; }")).toEqual([]);

  expect(inlineStyleColors(`<div style={{ color: "red" }} />`)).toEqual(["red"]);
  expect(inlineStyleColors(`<div style={{ "background-color": "#abc" }} />`)).toEqual(["#abc"]);
  expect(inlineStyleColors(`<div style={{ border: \`1px solid rgb(1,2,3)\` }} />`)).toEqual(["rgb("]);
  expect(inlineStyleColors(`<div style="background: black" />`)).toEqual(["black"]);
  expect(inlineStyleColors(`el.style.color = "white";`)).toEqual(["white"]);
  expect(inlineStyleColors(`el.style.setProperty("--x", "#123456");`)).toEqual(["#123456"]);

  expect(inlineStyleColors(`<div style={{ width: \`\${size}px\`, color: "var(--text-primary)" }} />`)).toEqual([]);
});
