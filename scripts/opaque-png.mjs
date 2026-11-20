// A PNG without its alpha channel: App Store Connect refuses an icon or a screenshot that has one,
// and Play wants its feature graphic and screenshots in 24-bit colour. Takes an 8-bit RGBA PNG (what
// Chromium and `cargo tauri icon` write) and returns the same pixels as RGB; any other PNG is
// returned as it is.
import zlib from 'node:zlib';

export function opaque(buf) {
  const width = buf.readUInt32BE(16);
  const height = buf.readUInt32BE(20);
  const bitDepth = buf[24];
  const colorType = buf[25];
  if (colorType !== 6 || bitDepth !== 8) {
    return buf;
  }
  let pos = 8;
  const idatParts = [];
  while (pos < buf.length) {
    const len = buf.readUInt32BE(pos);
    const type = buf.toString('ascii', pos + 4, pos + 8);
    if (type === 'IDAT') idatParts.push(buf.subarray(pos + 8, pos + 8 + len));
    pos += 12 + len;
  }
  const uncompressed = zlib.inflateSync(Buffer.concat(idatParts));
  const srcStride = 1 + width * 4;
  const dstStride = 1 + width * 3;
  const dstData = Buffer.alloc(dstStride * height);
  const rawRgba = Buffer.alloc(width * 4 * height);

  for (let y = 0; y < height; y++) {
    const filter = uncompressed[y * srcStride];
    const srcRow = uncompressed.subarray(y * srcStride + 1, (y + 1) * srcStride);
    const prevRow = y > 0 ? rawRgba.subarray((y - 1) * width * 4, y * width * 4) : null;
    const curRow = rawRgba.subarray(y * width * 4, (y + 1) * width * 4);

    for (let x = 0; x < width * 4; x++) {
      const a = x >= 4 ? curRow[x - 4] : 0;
      const b = prevRow ? prevRow[x] : 0;
      const c = prevRow && x >= 4 ? prevRow[x - 4] : 0;
      let val = srcRow[x];
      if (filter === 1) val = (val + a) & 0xff;
      else if (filter === 2) val = (val + b) & 0xff;
      else if (filter === 3) val = (val + Math.floor((a + b) / 2)) & 0xff;
      else if (filter === 4) {
        const p = a + b - c;
        const pa = Math.abs(p - a);
        const pb = Math.abs(p - b);
        const pc = Math.abs(p - c);
        const pr = pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
        val = (val + pr) & 0xff;
      }
      curRow[x] = val;
    }

    dstData[y * dstStride] = 0; // None filter
    for (let x = 0; x < width; x++) {
      dstData[y * dstStride + 1 + x * 3] = curRow[x * 4];
      dstData[y * dstStride + 1 + x * 3 + 1] = curRow[x * 4 + 1];
      dstData[y * dstStride + 1 + x * 3 + 2] = curRow[x * 4 + 2];
    }
  }

  const newCompressed = zlib.deflateSync(dstData);
  const crcTable = [];
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    crcTable[n] = c;
  }
  function crc32(b) {
    let crc = 0xffffffff;
    for (let i = 0; i < b.length; i++) crc = (crc >>> 8) ^ crcTable[(crc ^ b[i]) & 0xff];
    return (crc ^ 0xffffffff) >>> 0;
  }
  function makeChunk(type, data) {
    const chunk = Buffer.alloc(12 + data.length);
    chunk.writeUInt32BE(data.length, 0);
    chunk.write(type, 4);
    data.copy(chunk, 8);
    const typeAndData = chunk.subarray(4, 8 + data.length);
    chunk.writeUInt32BE(crc32(typeAndData), 8 + data.length);
    return chunk;
  }

  const sig = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 2; // RGB (no alpha)
  ihdr[10] = 0;
  ihdr[11] = 0;
  ihdr[12] = 0;

  return Buffer.concat([
    sig,
    makeChunk('IHDR', ihdr),
    makeChunk('IDAT', newCompressed),
    makeChunk('IEND', Buffer.alloc(0)),
  ]);
}
