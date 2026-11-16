#!/usr/bin/env python3
"""Regenerates the xlsx fixtures in this folder with openpyxl, then lets LibreOffice rewrite one of them in its own
style. Both are test oracles only: nothing in the suite depends on them. Run: python3 corpus/make_corpus.py"""
import datetime, os, shutil, subprocess, tempfile
from openpyxl import Workbook
from openpyxl.styles import Alignment, Border, Font, PatternFill, Side
from openpyxl.workbook.defined_name import DefinedName

here = os.path.dirname(os.path.abspath(__file__))
wb = Workbook()
ws = wb.active
ws.title = "Data"
ws.append(["Name", "Qty", "Price", "Total", "Date", "OK"])
ws.append(["bolt", 4, 1.5, "=B2*C2", datetime.date(2024, 2, 29), True])
ws.append(["nut, hex", 9, 0.25, "=B3*C3", datetime.date(2024, 3, 1), False])
ws.append(["  padded ", "=SUM(B2:B3)", "=AVERAGE(C2:C3)", "=SUM(D2:D3)", "=E3-E2", "=AND(F2,F3)"])
ws.append(["multi\nline", -0.0000001, None, None, None, None])
ws.append(['=_xlfn.XLOOKUP("nut, hex",A2:A3,B2:B3)', '=TEXT(E2,"dddd")', '=IF(B4>10,"big","small")', "=1/0", '=VLOOKUP("bolt",A2:D3,4,FALSE)', "=Other!B2+1"])
for r in (2, 3):
    ws[f"C{r}"].number_format = "#,##0.00"
    ws[f"E{r}"].number_format = "yyyy-mm-dd"
    ws[f"D{r}"].number_format = '"$"#,##0.00'
ws["E4"].number_format = "0"
ws["A1"].font = Font(bold=True)
ws["B1"].font = Font(bold=True, italic=True, underline="single", color="FFFF0000")
ws["C1"].fill = PatternFill("solid", fgColor="FFFFFF00")
ws["D1"].alignment = Alignment(horizontal="center", wrap_text=True)
ws["E1"].border = Border(bottom=Side(style="thin", color="FF000000"))
ws["F1"].alignment = Alignment(horizontal="right")
ws.column_dimensions["A"].width = 20
ws.row_dimensions[7].height = 30
ws.merge_cells("A8:C8")
ws["A8"] = "merged title"
other = wb.create_sheet("Other")
other["B2"] = "=Data!B4*2"
other["A1"] = "It's"
wb.defined_names["Total"] = DefinedName("Total", attr_text="Data!$D$4")
src = os.path.join(here, "openpyxl_basic.xlsx")
wb.save(src)
with tempfile.TemporaryDirectory() as d:
    subprocess.run(["soffice", "--headless", "--convert-to", "xlsx", "--outdir", d, src], check=True, capture_output=True)
    shutil.copy(os.path.join(d, "openpyxl_basic.xlsx"), os.path.join(here, "libreoffice_basic.xlsx"))
print("ok")
