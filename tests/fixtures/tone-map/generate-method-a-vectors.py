#!/usr/bin/env python3
"""Independent Decimal-70 evaluation of BT.2446-1 Tables 2/3 (not Rust output)."""
import decimal
import json

decimal.getcontext().prec = 70
D = decimal.Decimal
ZERO, ONE = D(0), D(1)
gamma = D("2.4")
rho_h = ONE + D(32) * (D(1000) / D(10000)) ** (ONE / gamma)
rho_s = ONE + D(32) * (D(100) / D(10000)) ** (ONE / gamma)


def evaluate(rgb):
    rp, gp, bp = [(D(v) / D(1000)) ** (ONE / gamma) for v in rgb]
    y = D(".2627") * rp + D(".6780") * gp + D(".0593") * bp
    if not y:
        return dict(rgb=["0"] * 3, ycbcr=["0"] * 3, mapped_luma="0")
    p = (ONE + (rho_h - ONE) * y).ln() / rho_h.ln()
    if p <= D(".7399"):
        c = D("1.0770") * p
    elif p < D(".9909"):
        c = -D("1.1510") * p * p + D("2.7811") * p - D(".6302")
    else:
        c = D(".5") * p + D(".5")
    ys = (rho_s ** c - ONE) / (rho_s - ONE)
    f = ys / (D("1.1") * y)
    cb = f * (bp - y) / D("1.8814")
    cr = f * (rp - y) / D("1.4746")
    yt = ys - max(D(".1") * cr, ZERO)
    r, b = yt + D("1.4746") * cr, yt + D("1.8814") * cb
    g = (yt - D(".2627") * r - D(".0593") * b) / D(".6780")
    return dict(rgb=list(map(str, (r, g, b))), ycbcr=list(map(str, (yt, cb, cr))), mapped_luma=str(ys))


samples = {f"neutral-{n}": [n] * 3 for n in ("0", ".0001", "1", "10", "100", "203", "400", "1000")}
samples.update(red=["1000", "0", "0"], green=["0", "1000", "0"], blue=["0", "0", "1000"],
               cyan=["0", "1000", "1000"], magenta=["1000", "0", "1000"], yellow=["1000", "1000", "0"],
               skin=["203", "120", "80"])
print(json.dumps({"standard": "ITU-R BT.2446-1 (03/2021) Tables 2/3; BT.2020-2 Table 4 inverse NCL",
                  "precision_decimal_digits": 70,
                  "vectors": [{"name": name, "input_nits": values, **evaluate(values)} for name, values in samples.items()]}, indent=2))
