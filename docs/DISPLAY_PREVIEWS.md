# Adafruit screen previews on HAL

In the shader Playtest, open **Esc → Settings → Display / Adafruit previews**.
Choose a panel and press Enter (or click it). The game keeps the current scene and uses the
panel's native render resolution, including the menu/HUD pixel grid, in a fixed borderless window.
The selection lasts for this session, including sector rebuilds.

| Panel | Native pixels | Visible area |
|---|---|---|
| [Adafruit #913](https://www.adafruit.com/product/913), 3.5 inch | 320 × 240 | 70.1 × 52.6 mm |
| [Adafruit #1591](https://www.adafruit.com/product/1591), 4.3 inch | 480 × 272 | 95.04 × 53.856 mm |
| [Adafruit #1678](https://www.adafruit.com/product/1678), 5 inch | 800 × 480 | 108 × 65 mm |

Dimensions are the visible screen, excluding bezels and boards. #1591 uses page 4 of the
[CH430WQ12A-T datasheet](https://cdn-shop.adafruit.com/product-files/1591/Datasheet.pdf).

KDE monitor metadata converts millimetres to physical output pixels; Winit handles desktop scaling.
HAL currently reports DP-3 as 5120 × 2880 over 595 × 335 mm, with 170% desktop scaling. At 100%
preview size the requested windows are respectively 603 × 452, 818 × 463 and 929 × 559 physical
output pixels. Fractional scaling can round the final window by a pixel or two.

**Physical size** adjusts both axes in 1% steps with Left/Right. Check the window width with a
ruler if precise sizing matters. If monitor dimensions are unavailable, the menu explicitly labels
the size as estimated and uses 96 PPI until calibrated. Scaling/display changes trigger a fresh
measurement. The preview simulates dimensions and resolution, not TFT colour, contrast, viewing
angles, composite-video softness, or Pi frame rate. Noninteger enlargement on HAL's pixel grid
can give neighbouring simulated pixels slightly different widths.

**Restore desktop window**, or **Alt+Enter**, restores the previous window mode and render
resolution. Esc backs out through Settings and Pause; Resume returns to the same game.

For direct launch, optionally prefix the normal shader launcher with
`KESSEL_DISPLAY_PREVIEW=913`, `1591`, or `1678`. The ordinary launcher remains desktop-sized.

Validation: the GPU integration test switches one live garage scene through all three sizes and
back to 640 × 360, retaining 982 meshes, 73 instance batches, camera position and scene state.
Native captures are under `captures/astra/display_preview_*_20260914.png`.
