"""Compare boards: each building-grammar preview beside the wartime photographs it must match.

Usage (from the repository root, after tools/worldgen/preview_blueprint.py has rendered the
previews into captures/):

    python3 tools/worldgen/grammar_boards.py captures \
        /home/alex/panzer/reference/wartime_photos_20260928 <board_dir>

writes <board_dir>/grammar_<typology>_<condition>.png. The photographs are read in place from the
reference set and never copied into captures/ or assets/. Which photographs sit beside which
preview, and what to check first, is the photo-reference analysis's table "Photographs to compare
renders against" (PHOTO_REFERENCE_ANALYSIS_20260928.md §4). Photograph ids [FF-NN] are the folder
number and the position within that folder in the set's manifest order.
"""
import json
import os
import sys

from PIL import Image, ImageDraw, ImageFont

TYPOLOGIES = ["merchant_brick_house", "apartment_block", "wooden_house", "workshop"]
CONDITIONS = ["intact", "burned", "shelled", "partly_collapsed"]

# (typology, condition) -> (photograph ids, check first), from the analysis's §4 table; "form"
# rows are compared with the intact preview.
COMPARE = {
    ("merchant_brick_house", "intact"): (["08-13", "07-14", "07-16"], "rusticated ground floor, tall windows with moulded heads, cornice"),
    ("merchant_brick_house", "burned"): (["07-14", "07-01", "04-01"], "roofless shell to the cornice, sky in every window"),
    ("merchant_brick_house", "shelled"): (["08-13", "04-05"], "pitting densest at ground floor, torn balcony railing"),
    ("merchant_brick_house", "partly_collapsed"): (["03-02", "04-12"], "corner keeping two walls, ragged stepped top"),
    ("apartment_block", "intact"): (["04-09", "04-11", "07-15"], "window grid proportions, raised ground floor, stair bay"),
    ("apartment_block", "burned"): (["04-09", "04-11", "07-01"], "full-height shell, concrete stair standing, stacks above walls"),
    ("apartment_block", "shelled"): (["07-15", "04-11", "01-13"], "grid intact, local holes, stepped breach at a corner"),
    ("apartment_block", "partly_collapsed"): (["07-16", "04-04", "04-12"], "slabs projecting or hanging, floor stubs, lone fragment"),
    ("wooden_house", "intact"): (["03-04", "06-05", "07-06"], "one storey on a plinth, 30 degree sheet roof, fences"),
    ("wooden_house", "burned"): (["04-27", "04-07", "04-23"], "stove and chimney only, at plot pitch"),
    ("wooden_house", "shelled"): (["03-04", "04-02", "07-12"], "missing boards, V-collapsed rafters"),
    ("wooden_house", "partly_collapsed"): (["03-04", "04-02", "07-12"], "missing boards, V-collapsed rafters"),
    ("workshop", "intact"): (["04-19", "08-12", "04-14"], "pier and bay, tall single storey, caving shed roofs"),
    ("workshop", "burned"): (["04-19", "08-12", "04-14"], "pier and bay, tall single storey, caving shed roofs"),
    ("workshop", "shelled"): (["04-19", "08-12", "04-14"], "pier and bay, tall single storey, caving shed roofs"),
    ("workshop", "partly_collapsed"): (["04-19", "08-12", "04-14"], "pier and bay, tall single storey, caving shed roofs"),
}

WIDTH = 2560
PHOTO_ROW_MAX_H = 700
GAP = 24
BACKGROUND = (28, 29, 31)
INK = (228, 228, 222)
DIM = (160, 160, 152)


def font(size, bold=False):
    for path in (
        "/usr/share/fonts/noto/NotoSans-Bold.ttf" if bold else "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf" if bold else "/usr/share/fonts/TTF/DejaVuSans.ttf",
    ):
        if os.path.isfile(path):
            return ImageFont.truetype(path, size)
    return ImageFont.load_default()


def photo_index(reference_dir):
    """[FF-NN] -> manifest record, numbering each folder's photographs in manifest order."""
    records = json.load(open(os.path.join(reference_dir, "manifest.json")))
    index, counters = {}, {}
    for record in records:
        path = record.get("file")
        if not path:
            continue
        folder = path.split("/")[0][:2]
        counters[folder] = counters.get(folder, 0) + 1
        index[f"{folder}-{counters[folder]:02d}"] = record
    return index


def fit_text(draw, text, face, width):
    """Shorten `text` with an ellipsis until it fits `width` pixels."""
    if draw.textlength(text, font=face) <= width:
        return text
    while text and draw.textlength(text + "...", font=face) > width:
        text = text[:-1]
    return text + "..."


def board(captures, reference_dir, out_dir, typology, condition, index):
    preview_path = os.path.join(captures, f"urban_grammar_{typology}_{condition}.png")
    preview = Image.open(preview_path).convert("RGB")
    preview = preview.resize((WIDTH, round(preview.height * WIDTH / preview.width)), Image.LANCZOS)
    ids, check = COMPARE[(typology, condition)]
    photos = []
    for pid in ids:
        record = index[pid]
        photos.append((pid, record, Image.open(os.path.join(reference_dir, record["file"])).convert("RGB")))
    # One row of photographs at a common height that fits the board's width.
    height = PHOTO_ROW_MAX_H
    total = sum(img.width * height / img.height for _, _, img in photos) + GAP * (len(photos) - 1)
    if total > WIDTH - 2 * GAP:
        height = height * (WIDTH - 2 * GAP) / total
    height = int(height)
    scaled = [(pid, record, img.resize((max(1, round(img.width * height / img.height)), height), Image.LANCZOS))
              for pid, record, img in photos]
    title_h, caption_h = 96, 92
    canvas = Image.new("RGB", (WIDTH, title_h + preview.height + GAP + height + caption_h), BACKGROUND)
    draw = ImageDraw.Draw(canvas)
    draw.text((GAP, 14), f"{typology} / {condition}", font=font(34, bold=True), fill=INK)
    draw.text(
        (GAP, 58),
        fit_text(draw, f"grammar preview: exterior from the street (left), cutaway without the street facade and roof (right). "
                       f"Compare: {', '.join('[' + p + ']' for p in ids)}. Check first: {check}.", font(22), WIDTH - 2 * GAP),
        font=font(22),
        fill=DIM,
    )
    canvas.paste(preview, (0, title_h))
    row_w = sum(img.width for _, _, img in scaled) + GAP * (len(scaled) - 1)
    x = (WIDTH - row_w) // 2
    y = title_h + preview.height + GAP
    for pid, record, img in scaled:
        canvas.paste(img, (x, y))
        label = f"[{pid}] {record.get('archive_id') or record.get('title', '')} ({record.get('date', '')})"
        credit = record.get("attribution") or record.get("photographer_agency", "")
        credit = f"{credit}, {record.get('licence', '')}".strip(", ")
        draw.text((x, y + height + 8), fit_text(draw, label, font(20, bold=True), img.width), font=font(20, bold=True), fill=INK)
        draw.text((x, y + height + 36), fit_text(draw, credit, font(17), img.width), font=font(17), fill=DIM)
        draw.text((x, y + height + 60), fit_text(draw, record.get("place", ""), font(17), img.width), font=font(17), fill=DIM)
        x += img.width + GAP
    os.makedirs(out_dir, exist_ok=True)
    out = os.path.join(out_dir, f"grammar_{typology}_{condition}.png")
    canvas.save(out)
    return out


def main():
    captures, reference_dir, out_dir = sys.argv[1], sys.argv[2], sys.argv[3]
    index = photo_index(reference_dir)
    for typology in TYPOLOGIES:
        for condition in CONDITIONS:
            print("[boards]", board(captures, reference_dir, out_dir, typology, condition, index))


if __name__ == "__main__":
    main()
