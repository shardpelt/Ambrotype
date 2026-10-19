# Panorama merge

Lightroom's Photo Merge > Panorama stitches overlapping photos into one wider photo. Its options are the projection (Spherical, Cylindrical, Perspective, or Auto Select), Auto Crop, Boundary Warp and Fill Edges. The result is a DNG named `-Pano` that edits like a raw, with lens profile corrections already applied (Adobe's help (maintainers' notes), Panoramas). Ours has the projections, Auto crop and the corrected DNG.

## Using it

- **Library:** select two or more overlapping raws, then **Merge to Panorama…** in the panel. Choose the projection, Auto crop, Lens corrections first and Auto tone, then Merge. The panorama is written next to the first photo as `<first>-Pano.dng` (`-Pano-2` and so on if that name is taken) and opens in the editor.
- **CLI:** `amb merge pano A.NEF B.NEF C.NEF [--out OUT.dng] [--projection auto|spherical|cylindrical|perspective] [--no-crop] [--no-lens] [--lens-db PATH] [--lcp-dir DIR] [--json]`. It prints the lens profile and brightness gain used for each photo, which photos overlap, the projection, the field of view and how closely matched points meet.
- **Shooting:** turn the camera about the lens (a nodal slide helps for close subjects), with about a third of each frame shared with the next. Keep the exposure and focal length the same.

## How it works (ours, not Adobe's)

After Brown & Lowe (2007), "Automatic Panoramic Image Stitching using Invariant Features". `amb_library::merge::pano` loads the frames, then `amb_engine::pano::stitch` aligns and renders them, on demosaiced camera RGB.

1. **Lens corrections first** (on by default): each frame's lens profile (your LCP files, then lensfun), for distortion, vignetting and colour fringes, zoomed so no blank edges remain. Stitching assumes a perfect pinhole camera, so distortion left in shows as seams. The lensfun database also gives the camera's crop factor.
2. **Upright:** each frame is turned as its EXIF orientation says, so portrait and landscape frames mix.
3. **Features:** Harris corners in a copy of each frame scaled to 1600 pixels on the long side, spread over the frame (adaptive non-maximal suppression, up to 1500), each with an oriented 8×8 patch descriptor (Brown, Szeliski & Winder, 2005). They are matched between every pair by nearest neighbour with a ratio test.
4. **Pair rotations:** for each pair, the rotation that maps one frame's rays onto the other's, by RANSAC on two matches at a time. A pair overlaps when more than 8 + 0.3 × its matches agree.
5. **Focal length:** a first guess from EXIF (focal length × crop factor against the 43.27 mm full-frame diagonal), refined by trying nearby values on the best pair. Without EXIF, a wide range is tried.
6. **Bundle adjustment:** starting rotations along the strongest links from the reference frame (the one with the most agreeing matches), then every rotation and the focal length refined together on all agreeing matches. This is Levenberg–Marquardt with a Huber loss, so a few bad matches don't pull. The focal length is held loosely to EXIF (σ 10% in ln f). Long lenses barely bend the frames, so the photos alone don't fix it there.
7. **Levelling** ("wave correction"): the world is turned so the frames' horizontal axes are level, so a sweep doesn't curve up or down.
8. **Projection:** Auto picks Perspective for fields up to about 100° × 90° (straight lines stay straight), Cylindrical up to 100° high (verticals stay straight), and Spherical beyond.
9. **Brightness:** a gain per frame so the overlaps agree, solved over every overlap with a pull towards 1 (Brown & Lowe's gain compensation). This evens out auto exposure and leftover vignetting.
10. **Blending** in two bands. The smooth part (a blur of about 0.6% of the frame) is averaged with weights that fall to zero at each frame's edges. The detail comes from the frame whose weight is highest. So seams neither ghost nor show a step. It runs in strips of 512 rows, so memory stays near the frames' own size.
11. **Auto crop:** the largest rectangle the frames cover. Without it, the uncovered corners are black.

## The DNG

The same writer as HDR merge ([hdr-merge.md](hdr-merge.md)): a floating-point linear DNG with the reference frame's colour matrix, white balance and EXIF. The lens make, model and focal length are left out. The frames are corrected already, and no lens drew the panorama's geometry, so a lens profile must not find it again (Lightroom also turns profile corrections off on its merged panoramas). Panoramas over 300 MP are scaled down to fit (Lightroom's limit is 512 MP, or 65,000 pixels on the long side).

## Checks

- Engine tests photograph a synthetic scene on a sphere with known rotations and focal length. Alignment recovers the focal length within 1.5% (from a hint 15% off) and the rotations within the same share. The stitch matches the scene to about 2% in every projection.
- The library test writes three frames as DNGs, the middle one stored on its side (EXIF orientation 6), and checks the stitched DNG's size, projection, field and missing lens tags.
- Real data: three views resampled from a 600 mm Canon R10 raw, each rolled differently, stitch level and seamless at 100%. Matched points meet within 0.5 px.

## Speed

Three 24 MP frames stitch into a 63 MP panorama in about 30 s in a release build on 4 cores, including decoding them and writing the DNG; memory peaks at about 2.6 GB.

## Open questions and gaps

- No Boundary Warp or Fill Edges, no HDR panorama, no "Create stack", and no preview in the dialog.
- Raws only: JPEGs aren't stitched yet.
- Lightroom's default is Spherical (with Auto Select Projection); ours is Auto. Its projection rules, blending and crop are unknown; compare in the trial month.
- The camera must turn about its lens: parallax (near things shifting against far ones) isn't handled, and shows as ghosting in the nearest objects.
- No frame is checked for a different exposure or focal length than the others; gains absorb small exposure differences only.
- Auto tone is our built-in Auto tone plugin, not Lightroom's Auto.
