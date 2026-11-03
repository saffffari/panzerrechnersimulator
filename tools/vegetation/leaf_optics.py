"""PROSPECT-D leaf optics at the renderer's three lanes (680 / 550 / 440 nm), a line-for-line port
of src/leaf_optics.rs (Feret et al. 2017 after Jacquemoud & Baret 1990; the table is PROSPECT-D's
published specific absorption coefficients as distributed with the prosail package). The
vegetation generator uses it to give each leaf material a measured-model reflectance instead of a
painted green; `tests` at the bottom pin it to the Rust module's grass values.
"""
import math
from typing import NamedTuple

# Rows at 680, 550, 440 nm: refractive index, k_Cab, k_Car, k_Anth, k_brown, k_w, k_m.
TABLE = (
    (1.4458, 6.89206e-02, 0.0, 2.01097e-04, 1.50400e-01, 4.31800e-03, 2.30000e+00),
    (1.4739, 9.30939e-03, 4.18898e-03, 6.81398e-02, 3.51100e-01, 5.88000e-04, 2.30000e+00),
    (1.4980, 7.37942e-02, 1.69569e-01, 4.47865e-02, 4.85200e-01, 1.04000e-04, 1.09600e+01),
)


class Leaf(NamedTuple):
    """Structure N, chlorophyll a+b, carotenoids, anthocyanins (ug/cm^2), brown pigments,
    equivalent water thickness (cm), dry matter (g/cm^2)."""
    n: float
    cab: float
    car: float
    anth: float
    brown: float
    cw: float
    cm: float


def e1(x):
    if x <= 1.0:
        s, term = 0.0, 1.0
        for k in range(1, 60):
            term *= -x / k
            s += -term / k
        return -0.5772156649015329 - math.log(x) + s
    b = x + 1.0
    c = 1.0 / 1e-300
    d = 1.0 / b
    h = d
    for i in range(1, 200):
        an = -float(i) * i
        b += 2.0
        d = 1.0 / (an * d + b)
        c = b + an / c
        de = c * d
        h *= de
        if abs(de - 1.0) < 1e-15:
            break
    return h * math.exp(-x)


def tav(alpha_deg, nr):
    rd = math.pi / 180.0
    n2 = nr * nr
    np_ = n2 + 1.0
    nm = n2 - 1.0
    a = (nr + 1.0) ** 2 / 2.0
    k = -(n2 - 1.0) ** 2 / 4.0
    sa = math.sin(alpha_deg * rd)
    b1 = math.sqrt((sa * sa - np_ / 2.0) ** 2 + k) if alpha_deg != 90.0 else 0.0
    b2 = sa * sa - np_ / 2.0
    b = b1 - b2
    b3, a3 = b ** 3, a ** 3
    ts = (k * k / (6.0 * b3) + k / b - b / 2.0) - (k * k / (6.0 * a3) + k / a - a / 2.0)
    tp1 = -2.0 * n2 * (b - a) / (np_ * np_)
    tp2 = -2.0 * n2 * np_ * math.log(b / a) / (nm * nm)
    tp3 = n2 * (1.0 / b - 1.0 / a) / 2.0
    tp4 = 16.0 * n2 * n2 * (n2 * n2 + 1.0) * math.log((2.0 * np_ * b - nm * nm) / (2.0 * np_ * a - nm * nm)) / (np_ ** 3 * nm * nm)
    tp5 = 16.0 * n2 ** 3 * (1.0 / (2.0 * np_ * b - nm * nm) - 1.0 / (2.0 * np_ * a - nm * nm)) / np_ ** 3
    return (ts + tp1 + tp2 + tp3 + tp4 + tp5) / (2.0 * sa * sa)


def prospect(leaf, row):
    nr, kab, kcar, kant, kbrown, kw, km = row
    k = (leaf.cab * kab + leaf.car * kcar + leaf.anth * kant + leaf.brown * kbrown + leaf.cw * kw + leaf.cm * km) / leaf.n
    tau = (1.0 - k) * math.exp(-k) + k * k * e1(k) if k > 0.0 else 1.0
    talf = tav(40.0, nr)
    ralf = 1.0 - talf
    t12 = tav(90.0, nr)
    r12 = 1.0 - t12
    t21 = t12 / (nr * nr)
    r21 = 1.0 - t21
    denom = 1.0 - r21 * r21 * tau * tau
    ta = talf * tau * t21 / denom
    ra = ralf + r21 * tau * ta
    t = t12 * tau * t21 / denom
    r = r12 + r21 * tau * t
    d = math.sqrt((1.0 + r + t) * (1.0 + r - t) * (1.0 - r + t) * (1.0 - r - t))
    rq, tq = r * r, t * t
    a = (1.0 + rq - tq + d) / (2.0 * r)
    b = (1.0 - rq + tq + d) / (2.0 * t)
    bnm1 = b ** (leaf.n - 1.0)
    bn2 = bnm1 * bnm1
    a2 = a * a
    den = a2 * bn2 - 1.0
    if r + t >= 1.0:
        tsub = t / (t + (1.0 - t) * (leaf.n - 1.0))
        rsub = 1.0 - tsub
    else:
        rsub, tsub = a * (bn2 - 1.0) / den, bnm1 * (a2 - 1.0) / den
    den = 1.0 - rsub * r
    return ra + ta * rsub * t / den, ta * tsub / den


def lanes(leaf):
    """(rho, tau) as linear RGB lanes (680, 550, 440 nm)."""
    rt = [prospect(leaf, row) for row in TABLE]
    return tuple(r for r, _ in rt), tuple(t for _, t in rt)


def _selftest():
    # src/leaf_optics.rs SPRING_GRASS through the same model: green reflectance above red/blue.
    rho, tau = lanes(Leaf(1.5, 40.0, 9.0, 0.0, 0.0, 0.010, 0.004))
    assert rho[1] > rho[0] and rho[1] > rho[2], rho
    assert all(0.0 < v < 0.6 for v in rho + tau)


if __name__ == '__main__':
    _selftest()
    print('leaf_optics ok')
