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

// The social preview: what a link to the repository or the sky unfurls into,
// and the picture GitHub shows for the repository. 1280x640 is GitHub's size.
//
// The banner is a plate with the artwork on its left: drawn as it is, the
// artwork would land off-centre and the plate's rounded corners would show as
// a seam against a background of the same colour. So the banner is laid on
// that colour first - the corners vanish into it - then trimmed down to the
// artwork, wherever the fonts installed here put the wordmark's end, and
// centred. Nothing about the banner's geometry is hard-coded: until v0.19 it
// cut the banner at a fixed 380 of its 720 units, and the underline beneath
// "a music universe" ran past that line on this machine's fonts and came out
// sliced off at the picture's edge.
const PLATE = "#1B2126";
const PREVIEW = { width: 1280, height: 640 };
// GitHub crops the edges in some views; keep the artwork inside this margin.
const PREVIEW_MARGIN = 140;

const bannerOnPlate = await sharp(BANNER, { density: 384 }).flatten({ background: PLATE }).png().toBuffer();
// The hex's own fill is a shade off the plate; a low threshold keeps it from
// counting as background while still eating the antialiased plate edge.
const artwork = await sharp(bannerOnPlate).trim({ background: PLATE, threshold: 4 }).png().toBuffer();
const fitted = await sharp(artwork)
  .resize({
    width: PREVIEW.width - 2 * PREVIEW_MARGIN,
    height: PREVIEW.height - 2 * PREVIEW_MARGIN,
    fit: "inside",
  })
  .png()
  .toBuffer();

await sharp({ create: { ...PREVIEW, channels: 4, background: PLATE } })
  .composite([{ input: fitted, gravity: "centre" }])
  .png()
  .toFile(path.join(ASSETS, "social-preview.png"));

// The SPA serves the same picture as the card a link unfurls into, so a link
// to lyrid in a chat shows the sky rather than a grey rectangle. Copied after
// it is written, not before: on a fresh checkout there is nothing there yet.
fs.copyFileSync(path.join(ASSETS, "social-preview.png"), path.join(WEB_PUBLIC, "social-preview.png"));
console.log("wrote social-preview.png");
