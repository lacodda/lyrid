// One-off: rasterize the lyrid brand SVGs into PNGs and a multi-size .ico.
// Run from this package so it resolves the local sharp install:
//   node export-assets.mjs
import sharp from "sharp";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ASSETS = path.resolve(HERE, "../../assets");
const PUBLIC = path.join(HERE, "public");
const SITE_ASSETS = path.join(HERE, "src/assets");
const WEB_PUBLIC = path.resolve(HERE, "../../web/public");

// Three masters, one per level of detail. Which one a raster comes from is
// decided by how big that raster is, never by what it is for: an .ico holds
// sizes from 16 to 256 and there is room in it for all three.
const S = path.join(ASSETS, "logo-s.svg"); // filled hex, bold code
const M = path.join(ASSETS, "logo-m.svg");
const L = path.join(ASSETS, "logo.svg"); // the full mark
const BANNER = path.join(ASSETS, "banner.svg");

/**
 * The master a raster of this size should be cut from.
 *
 * S is a hex filled with colour: at 16 px that is the only thing that reads,
 * and at 48 px it is a coloured blob where the mark should be. L is the full
 * mark: legible large, mud when it is small. The bands come from the line's
 * brand canon (`brand-line`), and the defect they exist to stop has shipped
 * twice in this line -- in kasl's docs header and on kilna's desktop, both
 * caught by the owner's eye rather than by a check.
 */
function levelFor(size) {
  if (size <= 27) return S;
  if (size <= 63) return M;
  return L;
}

// Largest first. Windows picks by nearest size and ignores the order, but
// tauri-codegen takes entries()[0] literally as the window icon -- so a 16 px
// entry at the front is a title bar stretched from sixteen pixels. lyrid is
// not a Tauri app, but the .ico is the line's shared artefact and the order
// is part of what makes it one.
const ICO_SIZES = [256, 128, 64, 48, 32, 24, 16];

async function png(src, size, out) {
  await sharp(src, { density: 384 }).resize(size, size).png().toFile(out);
}

// Minimal ICO container: header + directory entries + embedded PNG payloads.
function buildIco(pngBuffers, sizes) {
  const count = pngBuffers.length;
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0); // reserved
  header.writeUInt16LE(1, 2); // type: icon
  header.writeUInt16LE(count, 4);

  const entries = Buffer.alloc(16 * count);
  let offset = 6 + 16 * count;
  pngBuffers.forEach((buf, i) => {
    const size = sizes[i];
    const e = 16 * i;
    entries.writeUInt8(size >= 256 ? 0 : size, e + 0); // width (0 means 256)
    entries.writeUInt8(size >= 256 ? 0 : size, e + 1); // height
    entries.writeUInt8(0, e + 2); // palette
    entries.writeUInt8(0, e + 3); // reserved
    entries.writeUInt16LE(1, e + 4); // color planes
    entries.writeUInt16LE(32, e + 6); // bits per pixel
    entries.writeUInt32LE(buf.length, e + 8);
    entries.writeUInt32LE(offset, e + 12);
    offset += buf.length;
  });

  return Buffer.concat([header, entries, ...pngBuffers]);
}

fs.mkdirSync(PUBLIC, { recursive: true });
fs.mkdirSync(SITE_ASSETS, { recursive: true });

const icoParts = [];
for (const size of ICO_SIZES) {
  icoParts.push(await sharp(levelFor(size), { density: 384 }).resize(size, size).png().toBuffer());
}
fs.writeFileSync(path.join(ASSETS, "icon.ico"), buildIco(icoParts, ICO_SIZES));
console.log("wrote icon.ico");

// Favicon and docs logo. The site favicon is the SVG itself; the PNGs cover
// the platforms that will not take one.
await png(levelFor(32), 32, path.join(ASSETS, "favicon-32.png"));
// 180 px is a home-screen tile, not an icon in a list: the full mark.
await png(levelFor(180), 180, path.join(ASSETS, "apple-touch-icon.png"));
await png(levelFor(512), 512, path.join(ASSETS, "logo-512.png"));
fs.copyFileSync(path.join(ASSETS, "apple-touch-icon.png"), path.join(PUBLIC, "apple-touch-icon.png"));
fs.copyFileSync(S, path.join(PUBLIC, "favicon.svg"));
fs.copyFileSync(L, path.join(SITE_ASSETS, "logo.svg"));

// The SPA is a storefront too, and until now it had a favicon and nothing
// else -- no touch icon at all, and a header mark cut from the wrong level.
// Its files are written from the same masters by the same rule, so the two
// sites cannot drift apart.
fs.mkdirSync(WEB_PUBLIC, { recursive: true });
fs.copyFileSync(S, path.join(WEB_PUBLIC, "favicon.svg"));
fs.copyFileSync(path.join(ASSETS, "apple-touch-icon.png"), path.join(WEB_PUBLIC, "apple-touch-icon.png"));
// The header mark is 2.25rem -- 36 px, the M band.
fs.copyFileSync(M, path.join(WEB_PUBLIC, "mark.svg"));
console.log("wrote pngs");

// GitHub social preview: 1280x640. Two adjustments to the banner: its plate
// spans the full 720px while the artwork only fills the left ~380px (trim the
// tail, or it lands off-centre), and the rounded plate over an identical
// background leaves a visible seam (drop it and keep the inner rows only).
const bannerWidth = 1600;
const bannerHeight = Math.round((bannerWidth * 170) / 720);
const inset = Math.round((bannerWidth * 6) / 720); // clears the plate's rounded edge
const artworkWidth = Math.round((bannerWidth * 380) / 720);
const banner = await sharp(BANNER, { density: 384 })
  .resize({ width: bannerWidth })
  .extract({ left: inset, top: inset, width: artworkWidth - inset, height: bannerHeight - 2 * inset })
  .png()
  .toBuffer();

await sharp({
  create: { width: 1280, height: 640, channels: 4, background: "#1B2126" },
})
  .composite([{ input: await sharp(banner).resize({ width: 880 }).png().toBuffer(), gravity: "centre" }])
  .png()
  .toFile(path.join(ASSETS, "social-preview.png"));

// The SPA serves the same picture as the card a link unfurls into, so a link
// to lyrid in a chat shows the sky rather than a grey rectangle. Copied after
// it is written, not before: on a fresh checkout there is nothing there yet.
fs.copyFileSync(path.join(ASSETS, "social-preview.png"), path.join(WEB_PUBLIC, "social-preview.png"));
console.log("wrote social-preview.png");
