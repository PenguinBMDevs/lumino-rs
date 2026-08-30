import struct, zlib, json

PATH = "target/fast-release/domino_clip.bin"
data = open(PATH, "rb").read()
assert data[:18] == b"PortalSequenceData"
size_field = struct.unpack("<I", data[18:22])[0]
raw = zlib.decompress(data[22:])
assert len(raw) == size_field, (len(raw), size_field)


def parse_stream(b):
    """Parse flat [id:u8][kind:u8][len:u32 LE][payload] stream -> list of (id,kind,len,payload)."""
    out = []
    pos = 0
    while pos + 6 <= len(b):
        cid = b[pos]
        kind = b[pos + 1]
        ln = struct.unpack("<I", b[pos + 2:pos + 6])[0]
        if pos + 6 + ln > len(b):
            break
        out.append((cid, kind, ln, b[pos + 6:pos + 6 + ln]))
        pos += 6 + ln
    return out


top = parse_stream(raw)
print("TOP-LEVEL CHUNKS:", len(top))
for cid, kind, ln, p in top:
    tag = f"0x{cid:02x}"
    if 0x20 <= cid < 0x7f:
        tag += f"('{chr(cid)}')"
    print(f"  top id={tag} kind=0x{kind:02x} len={ln}")

# find eb chunk
eb = None
for cid, kind, ln, p in top:
    if cid == 0xEB and kind == 0x03:
        eb = p
assert eb is not None, "no eb chunk"

print("\n=== EB chunk inner stream ===")
eb_inner = parse_stream(eb)
print("eb records:", len(eb_inner))
for cid, kind, ln, p in eb_inner:
    tag = f"0x{cid:02x}"
    if 0x20 <= cid < 0x7f:
        tag += f"('{chr(cid)}')"
    print(f"  eb.id={tag} kind=0x{kind:02x} len={ln} payload_hex={p.hex()}")

# note records = eb_inner records with id==0xd1 kind==0x07
note_chunks = [(cid, kind, ln, p) for (cid, kind, ln, p) in eb_inner if cid == 0xD1 and kind == 0x07]
print(f"\n=== NOTE RECORDS (d1 07) count={len(note_chunks)} ===")
notes = []
for i, (cid, kind, ln, p) in enumerate(note_chunks):
    sub = parse_stream(p)  # inner fields of the note record
    rec = {"raw": p.hex()}
    for scid, skind, sln, sp in sub:
        key = f"0x{scid:02x}"
        if 0x20 <= scid < 0x7f:
            key += f"('{chr(scid)}')"
        val = sp.hex()
        if sln == 1 and 0x20 <= sp[0] < 0x7f:
            val += f" ('{chr(sp[0])}'={sp[0]})"
        if sln == 4:
            val += f" (u32={struct.unpack('<I', sp)[0]})"
        rec[f"field_{key}_kind{skind:02x}"] = val
        print(f"  note[{i}] field id={key} kind=0x{skind:02x} len={sln} -> {val}")
    notes.append(rec)

with open("domino_notes.json", "w") as f:
    json.dump(notes, f, indent=2)
print("\nwrote domino_notes.json")
