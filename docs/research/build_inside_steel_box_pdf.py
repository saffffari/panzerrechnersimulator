#!/usr/bin/env python3
"""Build the visual PDF edition of the primary-source report."""

from __future__ import annotations

import html
import os
import re
from pathlib import Path

from reportlab.graphics.shapes import Drawing, Line, Polygon, Rect, String
from reportlab.lib import colors
from reportlab.lib.colors import HexColor
from reportlab.lib.enums import TA_CENTER, TA_LEFT
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import ParagraphStyle, getSampleStyleSheet
from reportlab.lib.units import mm
from reportlab.pdfbase.pdfmetrics import stringWidth
from reportlab.platypus import (
    BaseDocTemplate,
    Flowable,
    Frame,
    HRFlowable,
    KeepTogether,
    ListFlowable,
    ListItem,
    PageBreak,
    PageTemplate,
    Paragraph,
    Spacer,
    Table,
    TableStyle,
)
from reportlab.platypus.tableofcontents import TableOfContents


ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "docs/research/inside-the-steel-box-report-source.md"
OUTPUT = ROOT / "output/pdf/inside-the-steel-box-firsthand-tank-crew-report.pdf"

PAGE_W, PAGE_H = A4
MARGIN_X = 17 * mm
MARGIN_TOP = 19 * mm
MARGIN_BOTTOM = 17 * mm

INK = HexColor("#1A1B1D")
PAPER = HexColor("#F4F0E7")
RED = HexColor("#8A2D2C")
RED_DARK = HexColor("#5B1E20")
STEEL = HexColor("#53606A")
STEEL_DARK = HexColor("#2E3740")
MUD = HexColor("#8A765B")
SAND = HexColor("#D8CDBA")
PALE_RED = HexColor("#EAD8D3")
PALE_STEEL = HexColor("#DCE3E5")
PALE_GOLD = HexColor("#E8DEC4")
WHITE = colors.white


def inline_markup(text: str) -> str:
    """Convert the small inline-Markdown subset used by the source."""
    tokens: list[str] = []

    def keep_link(match: re.Match[str]) -> str:
        label = html.escape(match.group(1))
        url = html.escape(match.group(2), quote=True)
        tokens.append(f'<link href="{url}" color="#8A2D2C"><u>{label}</u></link>')
        return f"@@TOKEN{len(tokens) - 1}@@"

    text = re.sub(r"\[([^\]]+)\]\((https?://[^)]+)\)", keep_link, text)
    text = html.escape(text)
    text = re.sub(r"\*\*([^*]+)\*\*", r"<b>\1</b>", text)
    text = re.sub(r"(?<!\*)\*([^*]+)\*(?!\*)", r"<i>\1</i>", text)
    for index, token in enumerate(tokens):
        text = text.replace(f"@@TOKEN{index}@@", token)
    return text


def wrap_label(text: str, width: float, font: str, size: float) -> list[str]:
    words = text.split()
    lines: list[str] = []
    line = ""
    for word in words:
        candidate = word if not line else f"{line} {word}"
        if stringWidth(candidate, font, size) <= width:
            line = candidate
        else:
            if line:
                lines.append(line)
            line = word
    if line:
        lines.append(line)
    return lines


class CrewMap(Flowable):
    def __init__(self, vehicle: str, subtitle: str, seats: list[tuple[str, str, str]]):
        super().__init__()
        self.vehicle = vehicle
        self.subtitle = subtitle
        self.seats = seats
        self.width = PAGE_W - 2 * MARGIN_X
        self.height = 100 * mm

    def draw(self) -> None:
        c = self.canv
        w, h = self.width, self.height
        c.saveState()
        c.setFillColor(STEEL_DARK)
        c.roundRect(0, 0, w, h, 3 * mm, fill=1, stroke=0)
        c.setFillColor(WHITE)
        c.setFont("Helvetica-Bold", 16)
        c.drawString(8 * mm, h - 12 * mm, self.vehicle)
        c.setFillColor(PALE_STEEL)
        c.setFont("Helvetica", 8.5)
        c.drawString(8 * mm, h - 18 * mm, self.subtitle)

        tank_x = 8 * mm
        tank_y = 18 * mm
        tank_w = 74 * mm
        tank_h = 53 * mm
        c.setFillColor(HexColor("#343F47"))
        c.setStrokeColor(HexColor("#AAB6BA"))
        c.setLineWidth(0.8)
        c.roundRect(tank_x, tank_y, tank_w, tank_h, 8 * mm, fill=1, stroke=1)
        c.setFillColor(HexColor("#44525C"))
        c.circle(tank_x + tank_w * 0.57, tank_y + tank_h * 0.57, 17 * mm, fill=1, stroke=1)
        c.setFillColor(HexColor("#65737B"))
        c.rect(tank_x + tank_w * 0.56, tank_y + tank_h * 0.55, 51 * mm, 4 * mm, fill=1, stroke=0)
        c.setStrokeColor(HexColor("#78878E"))
        c.line(tank_x + 5 * mm, tank_y + 8 * mm, tank_x + tank_w - 5 * mm, tank_y + 8 * mm)
        c.line(tank_x + 5 * mm, tank_y + tank_h - 8 * mm, tank_x + tank_w - 5 * mm, tank_y + tank_h - 8 * mm)

        positions = [
            (tank_x + 49 * mm, tank_y + 38 * mm),
            (tank_x + 40 * mm, tank_y + 29 * mm),
            (tank_x + 59 * mm, tank_y + 28 * mm),
            (tank_x + 20 * mm, tank_y + 28 * mm),
            (tank_x + 20 * mm, tank_y + 15 * mm),
        ]
        if len(self.seats) == 4:
            positions = [positions[0], positions[1], positions[3], positions[4]]
        if len(self.seats) == 5 and "CASemate" in self.subtitle.upper():
            positions = [
                (tank_x + 58 * mm, tank_y + 40 * mm),
                (tank_x + 52 * mm, tank_y + 28 * mm),
                (tank_x + 39 * mm, tank_y + 28 * mm),
                (tank_x + 20 * mm, tank_y + 30 * mm),
                (tank_x + 32 * mm, tank_y + 16 * mm),
            ]
        for index, (name, _, _) in enumerate(self.seats):
            x, y = positions[index]
            c.setFillColor(RED if index == 0 else MUD)
            c.circle(x, y, 3.3 * mm, fill=1, stroke=0)
            c.setFillColor(WHITE)
            c.setFont("Helvetica-Bold", 6.5)
            c.drawCentredString(x, y - 1.8, str(index + 1))

        card_x = 92 * mm
        card_w = w - card_x - 8 * mm
        card_h = 12.3 * mm
        for index, (name, view, witness) in enumerate(self.seats):
            y = h - 28 * mm - index * 13.5 * mm
            c.setFillColor(HexColor("#39464F") if index % 2 == 0 else HexColor("#32404A"))
            c.roundRect(card_x, y - card_h, card_w, card_h, 1.5 * mm, fill=1, stroke=0)
            c.setFillColor(RED if index == 0 else HexColor("#C2A977"))
            c.circle(card_x + 5 * mm, y - card_h / 2, 2.5 * mm, fill=1, stroke=0)
            c.setFillColor(WHITE)
            c.setFont("Helvetica-Bold", 8)
            c.drawString(card_x + 10 * mm, y - 4.4 * mm, name)
            c.setFillColor(PALE_STEEL)
            view_size = 6.3
            witness_size = 5.7
            witness_width = stringWidth(witness, "Helvetica-Oblique", witness_size)
            available_view = card_w - 17 * mm - witness_width
            fitted_view = view
            while stringWidth(fitted_view, "Helvetica", view_size) > available_view and " " in fitted_view:
                fitted_view = fitted_view.rsplit(" ", 1)[0]
            if fitted_view != view:
                fitted_view += "..."
            c.setFont("Helvetica", view_size)
            c.drawString(card_x + 10 * mm, y - 8.2 * mm, fitted_view)
            c.setFillColor(HexColor("#D8CDBA"))
            c.setFont("Helvetica-Oblique", witness_size)
            c.drawRightString(card_x + card_w - 3 * mm, y - 8.2 * mm, witness)
        c.restoreState()


class EvidenceLadder(Flowable):
    def __init__(self):
        super().__init__()
        self.width = PAGE_W - 2 * MARGIN_X
        self.height = 71 * mm

    def draw(self) -> None:
        c = self.canv
        w, h = self.width, self.height
        c.saveState()
        c.setFillColor(PAPER)
        c.setStrokeColor(SAND)
        c.roundRect(0, 0, w, h, 3 * mm, fill=1, stroke=1)
        c.setFillColor(INK)
        c.setFont("Helvetica-Bold", 13)
        c.drawString(7 * mm, h - 10 * mm, "Two questions for every account")
        c.setFont("Helvetica", 8.2)
        c.setFillColor(STEEL)
        c.drawString(7 * mm, h - 16 * mm, "Human experience and vehicle identification are rated independently.")

        labels = [
            ("A", "Wartime record", "closest in time; still shaped by purpose"),
            ("B", "Anchored memoir", "diary, letters, records, or named crew"),
            ("C", "Late oral history", "strongest on body and routine"),
            ("D", "Unverified retelling", "excluded from factual reconstruction"),
        ]
        x0 = 7 * mm
        box_w = (w - 19 * mm) / 4
        for i, (grade, title, note) in enumerate(labels):
            x = x0 + i * (box_w + 1.7 * mm)
            fill = [RED_DARK, STEEL_DARK, MUD, HexColor("#6E6861")][i]
            c.setFillColor(fill)
            c.roundRect(x, 23 * mm, box_w, 24 * mm, 2 * mm, fill=1, stroke=0)
            c.setFillColor(WHITE)
            c.setFont("Helvetica-Bold", 15)
            c.drawString(x + 4 * mm, 39 * mm, grade)
            c.setFont("Helvetica-Bold", 7.5)
            c.drawString(x + 4 * mm, 33.5 * mm, title)
            c.setFont("Helvetica", 6.2)
            lines = wrap_label(note, box_w - 8 * mm, "Helvetica", 6.2)
            for j, line in enumerate(lines[:2]):
                c.drawString(x + 4 * mm, 28.7 * mm - j * 3.1 * mm, line)

        c.setFillColor(PALE_RED)
        c.roundRect(7 * mm, 6 * mm, w - 14 * mm, 11 * mm, 1.5 * mm, fill=1, stroke=0)
        c.setFillColor(RED_DARK)
        c.setFont("Helvetica-Bold", 7.5)
        c.drawString(11 * mm, 12 * mm, "IDENTIFICATION")
        c.setFont("Helvetica", 7.2)
        c.drawString(39 * mm, 12 * mm, "confirmed  |  plausible  |  remembered-unverified  |  contradicted")
        c.restoreState()


class TimelineGraphic(Flowable):
    def __init__(self):
        super().__init__()
        self.width = PAGE_W - 2 * MARGIN_X
        self.height = 63 * mm

    def draw(self) -> None:
        c = self.canv
        w, h = self.width, self.height
        c.saveState()
        c.setFillColor(STEEL_DARK)
        c.roundRect(0, 0, w, h, 3 * mm, fill=1, stroke=0)
        c.setFillColor(WHITE)
        c.setFont("Helvetica-Bold", 13)
        c.drawString(7 * mm, h - 10 * mm, "Where the testimony sits in time")
        y = 24 * mm
        c.setStrokeColor(HexColor("#9AA8AE"))
        c.setLineWidth(1.2)
        c.line(10 * mm, y, w - 10 * mm, y)
        points = [
            (0.02, "1942", "Tiger debut\nnear Leningrad"),
            (0.16, "Jan 1943", "503 arrives south\nafter Winter Storm"),
            (0.34, "Jul 1943", "Kursk /\nProkhorovka"),
            (0.56, "1944", "SU-85 / ISU\nambush accounts"),
            (0.76, "Jan 1945", "Kielce /\nEast Prussia"),
            (0.96, "May 1945", "War ends;\nmemory begins"),
        ]
        span = w - 24 * mm
        for i, (t, date, label) in enumerate(points):
            x = 12 * mm + t * span
            c.setFillColor(RED if i in (1, 2, 4) else HexColor("#C4AD7D"))
            c.circle(x, y, 2.8 * mm, fill=1, stroke=0)
            c.setFillColor(WHITE)
            c.setFont("Helvetica-Bold", 7.2)
            c.drawCentredString(x, y + 7 * mm, date)
            c.setFillColor(PALE_STEEL)
            c.setFont("Helvetica", 5.9)
            for j, line in enumerate(label.split("\n")):
                c.drawCentredString(x, y - 7 * mm - j * 3.2 * mm, line)
        c.restoreState()


class SensoryMatrix(Flowable):
    def __init__(self):
        super().__init__()
        self.width = PAGE_W - 2 * MARGIN_X
        self.height = 87 * mm

    def draw(self) -> None:
        c = self.canv
        w, h = self.width, self.height
        c.saveState()
        c.setFillColor(PAPER)
        c.setStrokeColor(SAND)
        c.roundRect(0, 0, w, h, 3 * mm, fill=1, stroke=1)
        c.setFillColor(INK)
        c.setFont("Helvetica-Bold", 13)
        c.drawString(7 * mm, h - 10 * mm, "Sensory anatomy: agreement across opposing crews")
        rows = [
            ("SIGHT", "slits, tunnel optics", "smoke, dust, fog", "exposure to see"),
            ("SOUND", "engine and tracks", "gun concussion", "impact as giant blow"),
            ("SMELL", "fuel and oil", "propellant and hot metal", "blood and burned flesh"),
            ("TOUCH", "heat or frozen steel", "recoil and vibration", "spall, fire, escape"),
            ("TIME", "long maintenance", "seconds of violence", "years of return in sleep"),
        ]
        left = 7 * mm
        top = h - 19 * mm
        label_w = 22 * mm
        cell_w = (w - 14 * mm - label_w) / 3
        row_h = 11.2 * mm
        headers = ["BEFORE CONTACT", "DURING CONTACT", "AFTER / MEMORY"]
        for j, header in enumerate(headers):
            x = left + label_w + j * cell_w
            c.setFillColor(STEEL_DARK)
            c.rect(x, top, cell_w - 0.6 * mm, 8 * mm, fill=1, stroke=0)
            c.setFillColor(WHITE)
            c.setFont("Helvetica-Bold", 6.7)
            c.drawCentredString(x + cell_w / 2, top + 2.8 * mm, header)
        for i, row in enumerate(rows):
            y = top - (i + 1) * row_h
            c.setFillColor(RED_DARK if i % 2 == 0 else MUD)
            c.rect(left, y, label_w - 0.6 * mm, row_h - 0.6 * mm, fill=1, stroke=0)
            c.setFillColor(WHITE)
            c.setFont("Helvetica-Bold", 7.4)
            c.drawString(left + 3 * mm, y + 4 * mm, row[0])
            for j, text in enumerate(row[1:]):
                x = left + label_w + j * cell_w
                c.setFillColor(PALE_STEEL if (i + j) % 2 == 0 else PALE_GOLD)
                c.rect(x, y, cell_w - 0.6 * mm, row_h - 0.6 * mm, fill=1, stroke=0)
                c.setFillColor(INK)
                c.setFont("Helvetica", 6.7)
                lines = wrap_label(text, cell_w - 6 * mm, "Helvetica", 6.7)
                for k, line in enumerate(lines[:2]):
                    c.drawCentredString(x + cell_w / 2, y + 5.9 * mm - k * 3.3 * mm, line)
        c.restoreState()


class MythMatrix(Flowable):
    def __init__(self):
        super().__init__()
        self.width = PAGE_W - 2 * MARGIN_X
        self.height = 82 * mm

    def draw(self) -> None:
        c = self.canv
        w, h = self.width, self.height
        c.saveState()
        c.setFillColor(STEEL_DARK)
        c.roundRect(0, 0, w, h, 3 * mm, fill=1, stroke=0)
        c.setFillColor(WHITE)
        c.setFont("Helvetica-Bold", 13)
        c.drawString(7 * mm, h - 10 * mm, "Memory is evidence - but not an order of battle")
        items = [
            ("CONFIRMED", "Ogledow Tiger IIs", "vehicle capture + records"),
            ("PLAUSIBLE", "Zheleznov at Kielce", "heavy battalion in wider area"),
            ("UNVERIFIED", "Eroshenko at 300 m", "no date, place, or German unit"),
            ("CONTRADICTED", "Ferdinand at Prokhorovka", "deployed in northern Kursk sector"),
        ]
        bar_y = 25 * mm
        bar_x = 8 * mm
        bar_w = w - 16 * mm
        segment = bar_w / 4
        fills = [HexColor("#6D9278"), HexColor("#B0A15E"), HexColor("#A8754D"), RED]
        for i, (status, example, basis) in enumerate(items):
            x = bar_x + i * segment
            c.setFillColor(fills[i])
            c.rect(x, bar_y, segment - 0.8 * mm, 16 * mm, fill=1, stroke=0)
            c.setFillColor(WHITE)
            c.setFont("Helvetica-Bold", 7.2)
            c.drawCentredString(x + segment / 2, bar_y + 10 * mm, status)
            c.setFont("Helvetica", 6.4)
            c.drawCentredString(x + segment / 2, bar_y + 5.4 * mm, example)
            c.setFillColor(PALE_STEEL)
            c.setFont("Helvetica", 6.1)
            lines = wrap_label(basis, segment - 5 * mm, "Helvetica", 6.1)
            for j, line in enumerate(lines[:2]):
                c.drawCentredString(x + segment / 2, 17 * mm - j * 3.2 * mm, line)
        c.setFillColor(PALE_STEEL)
        c.setFont("Helvetica-Oblique", 7.3)
        c.drawString(8 * mm, 7 * mm, "Confidence can differ: an injury may be certain while the target model remains unknown.")
        c.restoreState()


class ResearchDocTemplate(BaseDocTemplate):
    def __init__(self, filename: str, **kwargs):
        super().__init__(filename, **kwargs)
        frame = Frame(
            MARGIN_X,
            MARGIN_BOTTOM,
            PAGE_W - 2 * MARGIN_X,
            PAGE_H - MARGIN_BOTTOM - MARGIN_TOP,
            id="normal",
            leftPadding=0,
            rightPadding=0,
            topPadding=0,
            bottomPadding=0,
        )
        self.addPageTemplates(
            [
                PageTemplate(id="cover", frames=[frame], onPage=self.draw_cover_page),
                PageTemplate(id="body", frames=[frame], onPage=self.draw_body_page),
            ]
        )
        self._section = "Inside the Steel Box"

    def beforePage(self):
        self.handle_nextPageTemplate("body")

    def draw_cover_page(self, canvas, doc):
        canvas.saveState()
        canvas.setFillColor(STEEL_DARK)
        canvas.rect(0, 0, PAGE_W, PAGE_H, fill=1, stroke=0)
        canvas.setFillColor(RED)
        canvas.rect(0, 0, 13 * mm, PAGE_H, fill=1, stroke=0)
        canvas.setFillColor(HexColor("#46545D"))
        canvas.circle(PAGE_W - 30 * mm, PAGE_H - 31 * mm, 43 * mm, fill=1, stroke=0)
        canvas.setFillColor(HexColor("#53626C"))
        canvas.circle(PAGE_W - 30 * mm, PAGE_H - 31 * mm, 31 * mm, fill=1, stroke=0)
        canvas.setStrokeColor(HexColor("#96A3A8"))
        canvas.setLineWidth(1)
        canvas.circle(PAGE_W - 30 * mm, PAGE_H - 31 * mm, 22 * mm, fill=0, stroke=1)
        canvas.line(PAGE_W - 30 * mm, PAGE_H - 31 * mm, PAGE_W + 17 * mm, PAGE_H - 31 * mm)
        canvas.setFillColor(HexColor("#38444C"))
        canvas.roundRect(25 * mm, 20 * mm, PAGE_W - 43 * mm, 33 * mm, 3 * mm, fill=1, stroke=0)
        canvas.setFillColor(PALE_STEEL)
        canvas.setFont("Helvetica", 7.5)
        canvas.drawString(31 * mm, 42 * mm, "PRIMARY-SOURCE-LED DEEP RESEARCH")
        canvas.setFillColor(WHITE)
        canvas.setFont("Helvetica-Bold", 9)
        canvas.drawString(31 * mm, 34 * mm, "GERMAN TIGER CREWS  |  SOVIET T-34 CREWS  |  SU / ISU CREWS")
        canvas.setFillColor(HexColor("#CBBEA5"))
        canvas.setFont("Helvetica", 7.3)
        canvas.drawString(31 * mm, 27 * mm, "Crew positions, lived experience, violence, memory, and myth")
        canvas.restoreState()

    def draw_body_page(self, canvas, doc):
        canvas.saveState()
        canvas.setFillColor(PAPER)
        canvas.rect(0, 0, PAGE_W, PAGE_H, fill=1, stroke=0)
        canvas.setStrokeColor(SAND)
        canvas.setLineWidth(0.6)
        canvas.line(MARGIN_X, PAGE_H - 12 * mm, PAGE_W - MARGIN_X, PAGE_H - 12 * mm)
        canvas.setFillColor(STEEL)
        canvas.setFont("Helvetica", 7)
        header = "Inside the Steel Box" if doc.page <= 4 else self._section[:78]
        canvas.drawString(MARGIN_X, PAGE_H - 9 * mm, header)
        canvas.setFillColor(RED_DARK)
        canvas.setFont("Helvetica-Bold", 7)
        canvas.drawRightString(PAGE_W - MARGIN_X, PAGE_H - 9 * mm, "PRIMARY-SOURCE REPORT")
        canvas.setStrokeColor(SAND)
        canvas.line(MARGIN_X, 11 * mm, PAGE_W - MARGIN_X, 11 * mm)
        canvas.setFillColor(STEEL)
        canvas.setFont("Helvetica", 7)
        canvas.drawString(MARGIN_X, 7 * mm, "Research current to 1 September 2026")
        canvas.drawRightString(PAGE_W - MARGIN_X, 7 * mm, str(doc.page))
        canvas.restoreState()

    def afterFlowable(self, flowable):
        if isinstance(flowable, Paragraph) and hasattr(flowable, "_outline_level"):
            level = flowable._outline_level
            text = flowable.getPlainText()
            key = f"h{level}-{abs(hash(text))}-{self.page}"
            self.canv.bookmarkPage(key)
            self.canv.addOutlineEntry(text, key, level=level, closed=False)
            self.notify("TOCEntry", (level, text, self.page, key))
            if level == 0:
                self._section = text


def make_styles():
    sample = getSampleStyleSheet()
    styles = {}
    styles["cover_title"] = ParagraphStyle(
        "CoverTitle",
        parent=sample["Title"],
        fontName="Helvetica-Bold",
        fontSize=31,
        leading=31,
        textColor=WHITE,
        alignment=TA_LEFT,
        spaceAfter=9 * mm,
    )
    styles["cover_subtitle"] = ParagraphStyle(
        "CoverSubtitle",
        parent=sample["Normal"],
        fontName="Helvetica",
        fontSize=13,
        leading=18,
        textColor=HexColor("#D8E0E2"),
        spaceAfter=7 * mm,
    )
    styles["cover_meta"] = ParagraphStyle(
        "CoverMeta",
        parent=sample["Normal"],
        fontName="Helvetica-Bold",
        fontSize=8.2,
        leading=12,
        textColor=HexColor("#CBBEA5"),
        spaceAfter=2 * mm,
    )
    styles["deck"] = ParagraphStyle(
        "Deck",
        parent=sample["BodyText"],
        fontName="Helvetica",
        fontSize=11.2,
        leading=16,
        textColor=WHITE,
        borderColor=HexColor("#596870"),
        borderWidth=0.7,
        borderPadding=7 * mm,
        backColor=HexColor("#34414A"),
        spaceBefore=14 * mm,
    )
    styles["h1"] = ParagraphStyle(
        "H1",
        parent=sample["Heading1"],
        fontName="Helvetica-Bold",
        fontSize=19,
        leading=23,
        textColor=RED_DARK,
        spaceBefore=7 * mm,
        spaceAfter=3.5 * mm,
        keepWithNext=True,
    )
    styles["h2"] = ParagraphStyle(
        "H2",
        parent=sample["Heading2"],
        fontName="Helvetica-Bold",
        fontSize=14.2,
        leading=18,
        textColor=STEEL_DARK,
        spaceBefore=5 * mm,
        spaceAfter=2.5 * mm,
        keepWithNext=True,
    )
    styles["h3"] = ParagraphStyle(
        "H3",
        parent=sample["Heading3"],
        fontName="Helvetica-Bold",
        fontSize=11.2,
        leading=14,
        textColor=RED,
        spaceBefore=3.8 * mm,
        spaceAfter=1.7 * mm,
        keepWithNext=True,
    )
    styles["body"] = ParagraphStyle(
        "Body",
        parent=sample["BodyText"],
        fontName="Helvetica",
        fontSize=9.15,
        leading=13.1,
        textColor=INK,
        alignment=TA_LEFT,
        spaceAfter=2.5 * mm,
        splitLongWords=False,
    )
    styles["bullet"] = ParagraphStyle(
        "Bullet",
        parent=styles["body"],
        leftIndent=4 * mm,
        firstLineIndent=0,
        bulletIndent=0,
        spaceAfter=1.3 * mm,
    )
    styles["callout"] = ParagraphStyle(
        "Callout",
        parent=styles["body"],
        fontName="Helvetica-Bold",
        fontSize=10.2,
        leading=14.5,
        textColor=RED_DARK,
        backColor=PALE_RED,
        borderColor=RED,
        borderWidth=0,
        borderPadding=5 * mm,
        leftIndent=0,
        rightIndent=0,
        spaceBefore=2 * mm,
        spaceAfter=5 * mm,
    )
    styles["small"] = ParagraphStyle(
        "Small",
        parent=styles["body"],
        fontSize=7.7,
        leading=10.5,
        textColor=STEEL,
    )
    return styles


def heading(text: str, level: int, styles, bookmark_level: int) -> Paragraph:
    p = Paragraph(inline_markup(text), styles[f"h{level}"])
    p._outline_level = bookmark_level
    return p


def build_story(source_text: str, styles) -> list:
    story: list = []

    # Cover content sits over the cover-page artwork.
    story.append(Spacer(1, 43 * mm))
    story.append(Paragraph("INSIDE THE<br/>STEEL BOX", styles["cover_title"]))
    story.append(
        Paragraph(
            "German Tiger crews and Soviet tankers who fought them,<br/>in their own remembered world",
            styles["cover_subtitle"],
        )
    )
    story.append(Paragraph("WRITTEN AND VISUAL HISTORICAL REPORT", styles["cover_meta"]))
    story.append(Paragraph("72 linked sources  |  30+ named witnesses  |  position-by-position analysis", styles["cover_meta"]))
    story.append(
        Paragraph(
            "A tank was simultaneously a weapon, a workplace, a shelter, a crew's shared home, and a possible crematorium.",
            styles["deck"],
        )
    )
    story.append(PageBreak())

    story.append(Paragraph("CONTENTS", styles["h1"]))
    story.append(Paragraph("Navigate by crew station, battlefield scene, sensory evidence, or source audit.", styles["body"]))
    toc = TableOfContents()
    toc.levelStyles = [
        ParagraphStyle(
            "TOC1", fontName="Helvetica-Bold", fontSize=8.2, leading=10.5, leftIndent=0, textColor=RED_DARK,
            spaceBefore=0.7 * mm,
        ),
        ParagraphStyle(
            "TOC2", fontName="Helvetica", fontSize=7.1, leading=8.7, leftIndent=7 * mm, textColor=STEEL_DARK,
        ),
    ]
    story.append(toc)
    story.append(Spacer(1, 5 * mm))
    story.append(EvidenceLadder())
    story.append(Spacer(1, 5 * mm))
    story.append(TimelineGraphic())
    story.append(PageBreak())

    lines = source_text.splitlines()
    # Source title and metadata are rebuilt on the cover.
    start = 0
    for idx, line in enumerate(lines):
        if line.strip() == "This is a history of human beings inside armored vehicles, not a celebration of weapons and not a catalogue of aces. It asks what each man could actually see from his station, what a hit sounded and felt like, how crews lived and maintained their machines, what they did to survive, what they did to other people, and where their memories do or do not survive documentary scrutiny.":
            start = idx
            break
    lines = lines[start:]

    paragraph_buffer: list[str] = []
    bullets: list[tuple[str, bool]] = []

    def flush_paragraph():
        nonlocal paragraph_buffer
        if paragraph_buffer:
            text = " ".join(s.strip() for s in paragraph_buffer).strip()
            if text:
                style = styles["callout"] if text.startswith("The central finding is simple:") else styles["body"]
                story.append(Paragraph(inline_markup(text), style))
            paragraph_buffer = []

    def flush_bullets():
        nonlocal bullets
        if bullets:
            items = []
            ordered = bullets[0][1]
            for text, _ in bullets:
                items.append(ListItem(Paragraph(inline_markup(text), styles["bullet"]), leftIndent=0))
            list_args = dict(
                bulletType="1" if ordered else "bullet",
                leftIndent=5 * mm,
                bulletFontName="Helvetica-Bold",
                bulletFontSize=7,
                bulletColor=RED,
                spaceAfter=2.5 * mm,
            )
            if ordered:
                list_args["start"] = 1
            story.append(ListFlowable(items, **list_args))
            bullets = []

    inserted = set()
    for raw in lines:
        line = raw.rstrip()
        stripped = line.strip()
        if not stripped:
            flush_paragraph()
            flush_bullets()
            continue

        if stripped.startswith("### "):
            flush_paragraph()
            flush_bullets()
            story.append(heading(stripped[4:], 3, styles, 1))
            continue
        if stripped.startswith("## "):
            flush_paragraph()
            flush_bullets()
            title = stripped[3:]
            if title.startswith("I. The Tiger crew") and "tiger" not in inserted:
                story.append(
                    CrewMap(
                        "TIGER I - FIVE STATIONS",
                        "TOP-DOWN SCHEMATIC / WHAT EACH POSITION COULD KNOW",
                        [
                            ("1  Commander", "widest view; must expose himself", "Rosen / Rubbel / Carius"),
                            ("2  Gunner", "magnified tunnel; target, lay, fire", "Niemann / Siegert"),
                            ("3  Loader", "breech and racks; little outside view", "Carius / Strohmer"),
                            ("4  Driver", "narrow ground view; angle and survival", "Eschrig / Fischer / Pliska"),
                            ("5  Radio / bow gun", "command net; poor bow aperture", "Peuker / evidence gap"),
                        ],
                    )
                )
                story.append(Spacer(1, 4 * mm))
                inserted.add("tiger")
            elif title.startswith("III. The Soviet T-34") and "t34" not in inserted:
                story.append(
                    CrewMap(
                        "T-34 - FOUR OR FIVE STATIONS",
                        "T-34-76 COMMANDER OFTEN ALSO GUNNER / T-34-85 ADDS A GUNNER",
                        [
                            ("1  Commander", "T-34-76: observe, lead, aim, fire", "Eroshenko / Otroschenkov"),
                            ("2  Loader", "rounds, breech, hot cases, escape", "Babich / Zheleznov crew"),
                            ("3  Driver", "slit or periscope; ground and boot signals", "Popov / Aria"),
                            ("4  Radio / bow gun", "engine noise; tiny firing aperture", "Kirichenko / Potapov"),
                        ],
                    )
                )
                story.append(Spacer(1, 4 * mm))
                inserted.add("t34")
            elif title.startswith("VI. Soviet self-propelled") and "spg" not in inserted:
                story.append(
                    CrewMap(
                        "SU / ISU - CASEMATE CREW",
                        "CASEMATE: HULL MOVEMENT BECOMES PART OF AIM / FIRING SMOKE CAN BLIND",
                        [
                            ("1  Commander", "often exposed to orient after firing", "Priklonsky / Stychinsky"),
                            ("2  Gunner", "powerful sight; narrow traverse", "Salov / Lependin crew"),
                            ("3  Loader", "heavy ammunition under time pressure", "Pereskokov / Semenov crew"),
                            ("4  Driver", "turns hull to aim; must reverse into cover", "Tsibin / Nikonov"),
                            ("5  Breech / radio", "recoil cycle, comms, close danger", "Shiyan / evidence gap"),
                        ],
                    )
                )
                story.append(Spacer(1, 4 * mm))
                inserted.add("spg")
            elif title.startswith("VII. A sensory anatomy") and "sensory" not in inserted:
                story.append(SensoryMatrix())
                story.append(Spacer(1, 4 * mm))
                inserted.add("sensory")
            elif title.startswith("X. Myth audit") and "myth" not in inserted:
                story.append(MythMatrix())
                story.append(Spacer(1, 4 * mm))
                inserted.add("myth")
            story.append(heading(title, 1, styles, 0))
            continue
        if stripped.startswith("# "):
            continue

        number_match = re.match(r"^(\d+)\.\s+(.*)$", stripped)
        if number_match:
            flush_paragraph()
            bullets.append((number_match.group(2), True))
            continue
        if stripped.startswith("- "):
            flush_paragraph()
            bullets.append((stripped[2:], False))
            continue

        paragraph_buffer.append(stripped)

    flush_paragraph()
    flush_bullets()
    return story


def build_pdf() -> None:
    source_text = SOURCE.read_text(encoding="utf-8")
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    styles = make_styles()
    doc = ResearchDocTemplate(
        str(OUTPUT),
        pagesize=A4,
        leftMargin=MARGIN_X,
        rightMargin=MARGIN_X,
        topMargin=MARGIN_TOP,
        bottomMargin=MARGIN_BOTTOM,
        title="Inside the Steel Box: German Tiger and Soviet Tank Crew Testimony",
        author="OpenAI Codex - primary-source-led historical research",
        subject="First-hand accounts of German Tiger, Soviet T-34, SU and ISU crews",
    )
    story = build_story(source_text, styles)
    doc.multiBuild(story)
    print(OUTPUT)


if __name__ == "__main__":
    build_pdf()
