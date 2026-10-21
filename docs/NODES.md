# MaterialCraft node reference

217 texture node types, 84 function-graph node types and 5 pattern-program node types. This page is generated from the node definitions in `src/graph.rs` (`materialcraft nodes` prints the same data). Parameters are listed after the colon; every parameter has a default, and the editor's Properties panel shows ranges and units.

Common to most nodes: **Output Size** (relative to input, relative to parent or absolute, with a power-of-two modifier), **Output format** (graph default, 8 / 16-bit integer, 16-bit float or 32-bit float; the graph default is set under Node Precision and is 32-bit float unless changed), `seed`, `contrast`, `invert` on generators, and the shared **disorder** group (`disorder`, `disorder_speed`, `anisotropy`, `anisotropy_angle`) and **tile offset** (`tile_x`, `tile_y`) on noises. Every node tiles seamlessly and works on non-square graphs; the cell-based generators (`perlin`, `fbm`, `clouds`, `voronoi`, `cells`, `tile_sampler`, `tile_generator`) also take `square_cells=1` to keep their cells square when the graph is not.

Nodes with several outputs: `path_sample` (position, width, height, side); `shape_extrude` (height, walls); `uber_emboss` (emboss, highlight, shadow); `snow_filter` (height, mask); `water_filter` (color, mask, height, roughness); `color_match` (output, mask); `quantize_color` (output, id, palette); `weave_generator` (height, mask_1, mask_2, opacity); `rgba_split` (r, g, b, a); `alpha_split` (rgb, alpha); `voronoi` (output, cell_id, edge, distance); `cells` (output, cell_id, distance, edge); `splatter` (output, id, center, size, luminance); `flood_fill_to_position` (position, x, y); `flood_fill_to_bbox_size` (size, width, height); `frequency_split` (low, high); `bent_normal` (normal, ao); `convex_concave` (convex, concave); `paving` (height, cell_id, gaps); `metal_rough_to_spec_gloss` (diffuse, specular, glossiness); `spec_gloss_to_metal_rough` (base_color, metallic, roughness); `material_blend` (base_color, normal, height, roughness, metallic, ao); `path_border` (height, mask); `path_track` (height, rails, sleepers); `path_cable` (height, mask, strand); `path_seam` (height, stitches). Connect the desired output socket to an input socket, or to an Output node to expose it as a graph output.

## Generators

| Node | Inputs | Description and parameters |
|---|---|---|
| `color` |  | constant colour: r g b a |
| `white_noise` |  | per-pixel random: seed distribution(0 uniform,1 gaussian,2 triangle) |
| `gaussian_noise` |  | gaussian-distributed per-pixel noise: seed scale contrast invert disorder disorder_speed anisotropy anisotropy_angle tile_x tile_y |
| `perlin` |  | perlin noise: scale scale_x scale_y seed disorder disorder_speed tile_x tile_y |
| `fbm` |  | fractal noise/clouds: scale octaves gain seed mode(0 fbm,1 ridged,2 turbulence) disorder disorder_speed anisotropy anisotropy_angle tile_x tile_y |
| `voronoi` |  | cellular: scale jitter seed mode(0 f1,1 cell id,2 edge,3 f2,4 f2-f1,5 f1*f2,6 f1/f2) aspect offset_x offset_y distortion distortion_scale distance_mode(0 euclid,1 manhattan,2 chebyshev,3 minkowski) minkowski edge_thickness rounded distance_scale color_seed_mode color_seed invert |
| `voronoi_fractal` |  | layered voronoi: scale jitter seed mode aspect offset_x offset_y distortion distortion_scale distance_mode minkowski edge_thickness rounded distance_scale min_level max_level roughness lacunarity opacity blend(0 add,1 max,2 min) color_seed_mode color_seed invert |
| `gradient` |  | gradient: mode(0 linear,1 radial) angle |
| `circle` |  | radius softness |
| `rect` |  | width height softness |
| `polygon` |  | sides radius softness rotation |
| `checker` |  | tiles |
| `bricks` |  | rows cols mortar bevel offset lum_var seed |
| `pixel_processor` | in, in2, in3, in4 | per-pixel expr / expr_g / expr_b / expr_a (string params), or fn="function" to run a function graph per pixel |
| `shape` |  | procedural shape: square, disc, paraboloid, gaussian, diamond, pyramid, half sphere, cone |
| `tile_sampler` | pattern, pattern2, patte | rn3,pattern4,pattern5,pattern6,mask,scale_map,rotation_map,displacement_map,color_map,distribution_map,background grid of pattern instances: x_amount y_amount size aspect size_mode scale scale_random size_random rotation rotation_random pattern_rotation symmetry_random position_random offset offset_mode offset_random global_offset_x global_offset_y color color_random lum_by_number lum_by_scale mask_threshold mask_invert mask_random mask_checker invert_mask blending opacity background reverse_order pattern_type pattern_specific pattern_distribution |
| `tile_generator` | pattern, pattern2, patte | rn3,pattern4,pattern5,pattern6,mask,scale_map,rotation_map,displacement_map,color_map,distribution_map,background brick-style tiles: x_amount y_amount size aspect size_mode scale scale_random size_random rotation rotation_random pattern_rotation symmetry_random position_random offset offset_mode offset_random global_offset_x global_offset_y color color_random lum_by_number lum_by_scale mask_checker mask_horizontal mask_vertical mask_random invert_mask blending opacity background reverse_order pattern_type pattern_specific pattern_distribution |
| `splatter` | pattern, pattern2, patte | rn3,pattern4,pattern5,pattern6 random scatter of a pattern: count grid_number scale scale_var aspect rotation rot_var pattern_rotation lum_var disorder pan_x pan_y symmetry_random gain output_min output_max background blending opacity pattern_type pattern_specific pattern_distribution |
| `splatter_circular` | pattern, pattern2, patte | rn3,pattern4,pattern5,pattern6 instances on one or more circles: number amount_random rings radius radius_random ring_radius_multiplier angle_random spiral_factor spread size size_random scale_by_number scale_by_ring rotation rotation_random center_orientation ring_rotation ring_rotation_random position_random color luminance_random lum_by_number lum_by_ring mask_random background blending opacity pattern_type pattern_specific pattern_distribution |
| `clouds` |  | soft cloud noise: scale seed contrast invert disorder disorder_speed anisotropy anisotropy_angle tile_x tile_y |
| `bnw_spots` |  | black and white spots: scale seed contrast invert variant disorder disorder_speed anisotropy anisotropy_angle tile_x tile_y |
| `crystal` |  | faceted crystal cells: scale disorder seed contrast invert variant |
| `directional_noise` |  | streak noise, seamless at any angle: scale stretch angle seed contrast invert disorder disorder_speed anisotropy anisotropy_angle tile_x tile_y |
| `grunge` |  | grunge masks: variant(0 concrete,1 scratches,2 spots,3 smudges) balance large_variation base_noisiness dirt_specks_opacity invert_dirt scratches_opacity sharpen quantity scratch_width scratch_length details coverage coverage_contrast disorder disorder_speed seed contrast invert |
| `plasma` |  | smooth plasma waves |
| `stripes` |  | stripes: count angle softness width tile(1 = snap to seamless angle, 0 = exact) |
| `weave` |  | woven threads: count width |
| `star` |  | star shape: points inner size softness rotation |
| `gradient_axial` |  | symmetric gradient: angle |
| `gradient_circular` |  | angular gradient: offset |
| `cells` | color | cellular noise: scale jitter seed variant(0 cells1,1 cells2,2 cells3,3 cells4) luminance_random color_source(0 random,1 pseudorandom,2 image) pseudorandom_seed edge_width hardness contrast invert disorder disorder_speed anisotropy anisotropy_angle tile_x tile_y |
| `fractal_sum` |  | sum of noise octaves: scale min_level max_level roughness seed mode(0 base,1 turbulence,2 creased) contrast invert disorder disorder_speed |
| `waveform` |  | tileable wave pattern: count angle warp warp_scale seed shape(0 sine,1 triangle,2 saw,3 square) contrast invert disorder disorder_speed |
| `gaussian_spots` |  | soft gaussian spots: scale size size_variation seed contrast invert disorder disorder_speed anisotropy anisotropy_angle tile_x tile_y |
| `triangle_grid` |  | grid of triangles: style(0 squares,1 jittered mesh) count border variation x_amount y_amount random_position_multiplier quincunx_offset_x quincunx_offset_y triangulation(0 fixed,1 alternate,2 random) output(0 smooth,1 flat,2 edge distance) height_random thin_lines distance_to_edge_thickness thin_line_color thin_line_background seed contrast invert |
| `grunge_map` |  | grunge map 1-24: map balance contrast brush_pattern cracks disorder coverage seed invert |
| `weave_generator` |  | woven threads with gaps: tile_x tile_y weave shape gap_x gap_y contrast invert |
| `starburst` |  | star-like rays: count rays sharpness size rotation rotation_random seed contrast invert |
| `gradient_reflected` |  | gradient that goes 0-1-0: count angle contrast invert |
| `fx_map` | in, in2, in3, in4 | run a pattern program: program="name" depth seed blend (0 max 1 add 2 over 3 min 4 multiply) color bg bg_r bg_g bg_b |
| `svg` |  | vector image: file="art.svg" fit (0 contain 1 stretch 2 cover) scale x y |
| `text` |  | text: text="..." font="face.ttf" (optional) size x y align (0 left 1 centre 2 right) spacing line_spacing rotation value wrap(width as a fraction of the image, 0 off) use_color color_r color_g color_b |
| `value_processor` | in, in2, in3, in4 | run fn="function" once; the result (float, float3 or float4) fills the image. The function can read image statistics |
| `blue_noise` |  | evenly spread grain with little low-frequency content: scale seed tightness contrast invert |
| `fibers` |  | random curved hair-like strands: count length thickness angle angle_variation curl taper brightness_variation seed contrast invert |
| `scratches` |  | random thin scratches: count length thickness angle angle_variation curl taper brightness_variation seed contrast invert |
| `caustics` |  | light patterns of water: scale octaves strength sharpness seed contrast invert |
| `liquid_noise` |  | swirling warped noise: scale octaves warp mode(0 liquid,1 marble) bands seed contrast invert |
| `perlin_3d` |  | slice of 3D noise that loops in depth: scale depth octaves gain seed contrast invert |
| `paving` |  | irregular paving stones (outputs: height, cell_id, gaps): scale jitter gap bevel height_variation dome seed contrast invert |
| `wire_mesh` |  | woven or chain-link wire: count_x count_y wire_width weave_depth mode(0 square,1 diamond) contrast invert |
| `path_stroke` |  | line along a spline (dashes, taper, profile 0 flat,1 round,2 bevel): file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit width softness profile taper_start taper_end dash_length dash_gap contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse |
| `path_fill` |  | inside of closed splines: file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit even_odd softness contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse |
| `path_distance` |  | distance to a spline: mode(0 falloff from the line,1 signed,2 inward gradient,3 outward gradient) range file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse |
| `path_border` |  | border or bevel along closed splines (outputs: height, mask): width side(0 inside,1 outside,2 both) profile(0 linear,1 round,2 smooth) flat file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse |
| `path_scatter` | in,mask | copies of a pattern along a spline: spacing scale scale_variation jitter_along jitter_across rotation(0 follow,1 fixed,2 random) angle angle_variation density opacity_variation blend(0 max,1 add) offset seed file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse |
| `path_track` |  | railway track along a spline (outputs: height, rails, sleepers): gauge rail_width sleeper_spacing sleeper_length sleeper_width rail_height sleeper_height file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse |
| `path_cable` |  | twisted cable along a spline (outputs: height, mask, strand): width strands strand_size twist file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse |
| `path_seam` |  | stitched seam along a spline (outputs: height, stitches): stitch_length stitch_gap stitch_width groove_width groove_depth stitch_height file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse |
| `path_splatter` | in,mask | several rows of copies of a pattern along a spline, thinned out by a mask: rows row_spacing spacing scale scale_variation jitter_along jitter_across rotation(0 follow,1 fixed,2 random) angle angle_variation density opacity_variation blend(0 max,1 add) offset seed file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse |
| `path_sample` |  | maps from a spline (outputs: position, width, height, side): width file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse contrast invert |
| `herringbone` |  | herringbone pavement: scale gap bevel variation mode(0 height,1 brick id,2 orientation) seed contrast invert |
| `arc_pavement` |  | fan-shaped arc pavement: scale rings stones gap bevel variation mode(0 height,1 stone id) seed contrast invert |
| `anisotropic_noise` |  | noise stretched along any direction, still tiling: scale stretch angle octaves gain seed contrast invert |
| `fur_noise` |  | fur-like hairs: count length thickness angle angle_variation curl taper root_shade brightness_variation seed contrast invert |
| `moisture_noise` |  | damp patches with drips: scale coverage softness drips drip_length seed contrast invert |
| `microscope_noise` |  | cells seen through a microscope: scale jitter membrane nucleus grain mode(0 combined,1 membrane,2 nuclei,3 cell id) seed contrast invert |
| `worley_3d` |  | cellular noise in 3D, one slice per z: scale z jitter mode(0 f1,1 f2,2 f2-f1,3 cell id) seed contrast invert |
| `fbm_3d` |  | fractal noise in 3D, one slice per z: scale z octaves gain mode(0 fbm,1 ridged,2 turbulence) seed contrast invert |
| `volume_mask` |  | slice through a repeated 3D shape as a mask: shape(0 sphere,1 box,2 cylinder,3 torus,4 cone) size repeat z roundness softness contrast invert |
| `gradient_3d` |  | distance gradient of a repeated 3D shape slice: shape size repeat z roundness range contrast invert |

## SDF

| Node | Inputs | Description and parameters |
|---|---|---|
| `sdf_shape` |  | 2D signed distance shape: shape(0 circle,1 box,2 polygon,3 ring) size aspect sides rotation round x y range |
| `sdf_op` | a, b | combine SDFs: op(0 union,1 subtract,2 intersect) smooth range |
| `sdf_mask` | in | SDF -> mask: offset softness |
| `sdf_form` |  | procedural 3D SDF form as height: shape(0 sphere,1 box,2 torus,3 cylinder) size repeat round scatter seed height |

## Utility

| Node | Inputs | Description and parameters |
|---|---|---|
| `bitmap` |  | flip_x flip_y gray; imported bitmap (PNG, JPEG, TIFF, EXR, HDR, WebP, DDS, TGA, BMP): file="image.png" |
| `baked_map` |  | map baked from meshes and stored in the package: set="name" map="normal" flip_y invert gray; see the mesh baker |
| `input` |  | subgraph input: idx |
| `subgraph` | in0, in1, in2, in3 | instance of a reusable subgraph: graph="name" |
| `output` | in | graph output: usage identifier |
| `dot` | in | re-route a link |
| `input_value` |  | graph parameter read by functions: name, type (0 float, 1 int, 2 bool, 3 float2, 4 float3, 5 float4, 6 int2, 7 int3, 8 int4, 9 string), v / x y z w, min max, label |

## Filters

| Node | Inputs | Description and parameters |
|---|---|---|
| `switch` | a, b, c, d | pass one of up to four inputs: index |
| `flood_fill` | in | label connected regions of a mask |
| `flood_fill_to_index` | in | region number of each flood-fill region as a gray level |
| `flood_fill_to_position` | in | centre of each flood-fill region (outputs: position, x, y) |
| `flood_fill_to_bbox_size` | in | bounding-box size of each flood-fill region (outputs: size, width, height) |
| `flood_fill_random` | in | random grayscale per flood-fill region |
| `flood_fill_color` | in | random color per flood-fill region |
| `flood_fill_gradient` | in | gradient across each flood-fill region |
| `vector_warp` | in, vector | displace by a vector map: intensity directx |
| `slope_blur` | in, slope | blur along a slope map: samples intensity mode |
| `blur_hq` | in | high quality blur: intensity quality |
| `histogram_range` | in | select a range of values: position range contrast invert |
| `bevel` | in | bevel a shape: distance smoothing threshold use_as_normal normal_intensity corner(0 round,1 angular) directx |
| `distance` | in | distance field of a mask: distance mode |
| `color_match` | in, reference, src_color | ,target_color recolour: method(0 statistics transfer from reference,1 colour swap) strength source_mode(0 average,1 colour,2 input) src_r src_g src_b target_mode(0 colour,1 input) target_r target_g target_b custom_variation variation_hue variation_chroma variation_luma use_mask mask_hue_range mask_chroma_range mask_luma_range mask_smoothness |
| `make_it_tile_photo` | in | make a photo tile: width width_v |
| `make_it_tile_patch` | in | make an image tile with random patches |
| `clamp` | in | clamp values: low high apply_alpha |
| `highpass` | in | high-pass filter: radius |
| `swirl` | in | swirl distortion: amount radius |
| `blur` | in | gaussian blur: radius |
| `directional_blur` | in | angle length |
| `sharpen` | in | radius amount |
| `levels` | in | low high gamma out_low out_high |
| `adjust` | in | brightness contrast |
| `invert` | in | invert colours: invert |
| `threshold` | in | binary threshold: threshold softness mode(0 greater,1 greater or equal,2 lower,3 lower or equal) |
| `histogram_scan` | in | position width |
| `quantize` | in | steps |
| `to_gray` | in | luminance |
| `channel` | in | ch(0 r,1 g,2 b,3 a) |
| `merge` | r, g, b, a | combine channels |
| `tint` | in | multiply by colour: r g b |
| `colorize` | in | gradient map: interp(0 linear,1 smooth,2 step,3 smoother) smoothness addressing(0 clamp,1 repeat) use_alpha  stops p0 r0 g0 b0 a0 ... p15 r15 g15 b15 a15 |
| `hsl` | in | hue sat val |
| `transform` | in | offset_x offset_y scale_x scale_y rotation(turns) |
| `mirror` | in | mirror an image: axis(0 x,1 y,2 both,3 corner) offset_x offset_y invert_x invert_y corner(0 top left,1 top right,2 bottom left,3 bottom right) |
| `warp` | in, warp | slope warp: intensity filtering(0 bilinear,1 nearest,2 bicubic) |
| `directional_warp` | in, warp | warp in one direction: angle intensity offset filtering(0 bilinear,1 nearest,2 bicubic) |
| `blend` | fg, bg, mask | blend two images: mode(0 copy,1 add,2 sub,3 mul,4 screen,5 overlay,6 max,7 min,8 diff,9 softlight,20 add sub,21 switch) opacity alpha_mode(0 source alpha,1 ignore alpha,2 straight,3 premultiplied) crop_x0 crop_y0 crop_x1 crop_y1 |
| `quantize_color` | in | reduce to few colours (k-means): color_amount contour_smoothing dithering dithering_pattern(0 noise,1 bayer) ignore_alpha distance_color_space(0 lab,1 rgb) |
| `edge_detect` | in | edge lines: mode(0 lines,1 sobel) edge_width edge_roundness invert tolerance intensity |
| `emboss` | in | angle intensity |
| `non_uniform_blur` | in, mask | blur by a mask: intensity |
| `radial_blur` | in | spin blur around a centre: mode(0 spin,1 zoom) samples angle x y amount radius |
| `anisotropic_blur` | in | blur stretched in one direction: intensity anisotropy angle |
| `symmetry` | in | mirror the image around a line: axis(0 x,1 y,2 both,3 diagonal,4 anti-diagonal,5 eight-fold) offset flip transfer(0 copy,1 add,2 sub,3 mul,4 add sub,5 max,6 min) |
| `skew` | in | shear that keeps tiling: skew_x skew_y (whole numbers) align(0 centre,1 top left,2 bottom right) |
| `auto_levels` | in | stretch values to the full range: per_channel |
| `channel_mixer` | in | mix colour channels: rr rg rb gr gg gb br bg bb mono |
| `alpha_merge` | rgb, alpha | put a grayscale image into the alpha channel |
| `color_to_mask` | in | mask of pixels close to a colour: r g b range softness |
| `replace_color` | in | replace one colour with another: src_r src_g src_b dst_r dst_g dst_b range softness |
| `pow` | in | raise values to a power: exponent |
| `histogram_shift` | in | shift all values: position |
| `grayscale_conversion` | in | weighted grayscale: r g b |
| `sobel` | in | sobel edge magnitude: intensity |
| `glow` | in | glow around bright areas: width intensity clear glow_r glow_g glow_b |
| `mosaic` | in, mosaic_map | mosaic of the map regions (or a pixel grid without a map): count intensity |
| `non_uniform_directional_warp` | in, intensity, angle | warp along a direction chosen per pixel: intensity angle |
| `multi_directional_warp` | in, warp | warp in several directions at once: intensity directions(0 one,1 two,2 four) angle mode(0 average,1 max,2 min,3 chain) |
| `rgba_split` | in | split into R, G, B and A grayscale outputs |
| `alpha_split` | in | split into an RGB output and an alpha output |
| `shape_stroke` | in | outline of a shape: width offset softness threshold opacity color_r color_g color_b |
| `shape_glow` | in | glow around a shape: width softness intensity threshold opacity spread color_r color_g color_b |
| `chrominance_extract` | in | colour without brightness |
| `shape_drop_shadow` | in | drop shadow of a mask: angle distance blur opacity spread |
| `curve` | in | tone curves: points (master), points_r points_g points_b points_a as "x:y,x:y" lists |
| `channel_shuffle` | in, in2 | rebuild channels: src_r src_g src_b src_a (0-3 channels of in, 4-7 channels of in2, 8 zero, 9 one), inv_r inv_g inv_b inv_a |
| `contrast_luminosity` | in | contrast around mid-gray and brightness shift: contrast luminosity |
| `srgb_to_linear` | in | decode the sRGB curve (colour channels only) |
| `linear_to_srgb` | in | encode the sRGB curve (colour channels only) |
| `premultiply` | in | alpha premultiplication: mode(0 straight to premultiplied,1 premultiplied to straight) |
| `histogram_equalize` | in | spread values evenly over 0..1: amount per_channel |
| `min_max` | a, b | per-channel minimum or maximum of two images: mode(0 min,1 max) |
| `morphology` | in | grow or shrink bright areas: mode(0 dilate,1 erode,2 open,3 close) radius |
| `multi_switch` | a, b, c, d, e, f, g, h | pass one of up to eight inputs: index |
| `color_lut` | in | colour grade through a lookup table file (.cube, .3dl, or a strip / Hald image): file="grade.cube" intensity |
| `lookup_table` | in, lut | recolour through a lookup image: mode(0 per channel,1 by luminance) row |
| `frequency_split` | in | split into a blurred low band and a high band centred on gray (outputs: low, high): radius |
| `frequency_merge` | low, high | recombine the bands of frequency_split: gain |
| `auto_crop` | in | stretch the non-black area to fill the image: threshold margin keep_aspect |
| `polar_convert` | in | mode(0 cartesian to polar,1 polar to cartesian) radius rotation tile |
| `clone_patch` | in | copy a patch from one place to another: src_x src_y dst_x dst_y width height feather rotation opacity |
| `noise_upscale` | in | smoothly enlarge a coarse image: cells mode(0 bicubic,1 bilinear,2 nearest) |
| `symmetry_slice` | in | kaleidoscope of one slice around the centre: slices mirror rotation tile |
| `quad_transform` | in | move the four corners: tl_x tl_y tr_x tr_y br_x br_y bl_x bl_y tile |
| `trapezoid_transform` | in | perspective taper: top bottom skew tile |
| `palette_extract` | in | strip of the dominant colours of an image: count |
| `palette_apply` | in, palette | snap colours to the nearest palette colour: count amount |
| `make_it_tile_mirror` | in | make an image tile by mirroring: mode(0 both axes,1 horizontal,2 vertical) |
| `halftone` | in | dots whose size follows brightness: cells mode(0 grid,1 diagonal) softness gain |
| `reaction_diffusion` | in | growing spots and stripes seeded by the input: resolution iterations feed kill spots seed |
| `fxaa` | in | smooth jagged edges: strength threshold |
| `bilateral_blur` | in, guide | blur that keeps edges: radius range |
| `path_warp` | in | push, slide or swirl a picture near a spline: width amount mode(0 push away,1 slide along,2 swirl) file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse |
| `shape_extrude` | in | extrude shapes along a direction (outputs: height, walls): length angle top falloff contrast invert |
| `shape_mapper` | in,pattern | put a pattern into every flood-filled region: scale rotation_random flip_random keep_aspect seed contrast invert |
| `cross_section` | in | plot of the values along one row: position height thickness fill contrast invert |
| `uber_emboss` | height,mask,gloss | emboss with gloss (outputs: emboss, highlight, shadow): intensity angle elevation gloss highlight gain |
| `diffusion` | in,mask | fill unknown pixels from the known ones (alpha or mask): smoothing gray |
| `summed_area_table` | in | running sums of the image: per_channel normalize |
| `vector_morph` | in,vector | move pixels along a vector map and fill the holes: amount directx |
| `safe_transform` | in | transform that stays seamless: mode(0 tile-safe,1 free) angle scale_x scale_y offset_x offset_y filter(0 nearest,1 bilinear,2 bicubic) edge(0 repeat,1 mirror,2 clamp,3 empty) rotate_after_scale |

## Material

| Node | Inputs | Description and parameters |
|---|---|---|
| `curvature_smooth` | in | smooth curvature from a normal map |
| `height_blend` | bg, fg, mask | blend two height maps: opacity contrast output |
| `edge_wear` | in, noise | edge wear mask from a height map: amount width contrast |
| `dirt` | in, noise | dirt mask from an ambient occlusion map: amount contrast |
| `dust` | in, noise | dust mask from a normal map: amount contrast |
| `mesh_weathering` | curvature, ao, normal, noise | wear, dirt and dust masks from baked maps: wear dirt dust up_angle(degrees) up_spread breakup scale contrast invert seed |
| `normal` | in | height->normal (OpenGL +Y): strength directx alpha_from_input |
| `normal_blend` | a, b, mask | blend normal maps (a = background, b = foreground): opacity |
| `ao` | in | ambient occlusion: mode(0 blur,1 horizon) radius intensity hbao_radius hbao_intensity quality(0 4,1 8,2 16 samples) use_world_units height_depth surface_size height_depth_cm |
| `curvature` | in | curvature of a normal map: mode(0 normal map,1 height map) intensity directx radius |
| `normal_invert` | in | flip normal map channels: invert_x invert_y invert_z invert_a |
| `normal_normalize` | in | renormalise a normal map |
| `normal_vector_rotation` | in, rotation | rotate normal vectors: angle directx |
| `normal_to_height` | in | integrate a normal map into a height map: directx relief_balance opacity |
| `normal_combine` | base, detail | combine two normal maps: technique(0 whiteout,1 channel mixer,2 detail oriented) |
| `bent_normal` | in | bent normal of a height map (outputs: normal, ao): radius relief directions steps directx |
| `normal_facing` | in | mask of surfaces that face a direction: angle tilt sharpness |
| `height_normal_blend` | normal, height | add the relief of a height map to a normal map: strength mode(0 reoriented,1 whiteout,2 partial derivative) opacity directx |
| `height_to_normal_world` | in | normal map from real-world sizes: size height directx |
| `normal_transform` | in | transform a normal map and turn its vectors with it: offset_x offset_y scale_x scale_y rotation rotate_vectors directx |
| `normal_sobel` | in | edges or curvature of a normal map: mode(0 edges,1 curvature) intensity directx |
| `metal_rough_to_spec_gloss` | base_color, metallic, roughness | convert a metal / roughness material (outputs: diffuse, specular, glossiness) |
| `spec_gloss_to_metal_rough` | diffuse, specular, glossiness | convert a specular / glossiness material (outputs: base_color, metallic, roughness) |
| `material_blend` | bg_base, bg_normal, bg_height, bg_rough, bg_metal, bg_ao, fg_base, fg_normal, fg_height, fg_rough, fg_metal, fg_ao, mask | blend two complete materials (outputs: base_color, normal, height, roughness, metallic, ao): opacity height_influence sharpness |
| `height_combine` | a, b, mask | combine two height maps: mode(0 max,1 min,2 add,3 difference) amount |
| `convex_concave` | in | ridge and cavity masks of a height map (outputs: convex, concave): radius gain |
| `leak_streaks` | in, noise | streaks that run on from a mask like drips: length angle intensity breakup |
| `settled_dust` | in, noise | dust that settles on flat upward surfaces and in cavities: amount slope cavity relief radius |
| `snow_filter` | height,mask | snow on upward-facing surfaces (outputs: height, mask): coverage slope softness breakup height_bias depth steepness scale seed |
| `season_filter` | in,mask | recolour foliage for a season (0 spring,1 summer,2 autumn,3 winter): season strength variation foliage_hue foliage_range scale seed |
| `water_filter` | height,base | water in the low parts (outputs: color, mask, height, roughness): level softness tint_r tint_g tint_b murkiness wet_edge ripples ripple_scale dry_roughness wet_roughness seed |

## Function-graph nodes

Used inside graphs with `meta kind "function"`. Links carry values (float, float2-4, integer, integer2-4, boolean, string). Slots `a`, `b`, `c` are operands. Running a function never produces NaN or infinity.

| Node | Inputs | Description and parameters |
|---|---|---|
| `f_out` | in | result of the function |
| `f_pin` | a | re-route a value link |
| `f_float` |  | constant float: v |
| `f_int` |  | constant integer: v |
| `f_bool` |  | constant boolean: v (0 or 1) |
| `f_string` |  | constant string: v |
| `f_vec2` | x, y | float2 from parts (x y, linked parts win over the typed values) |
| `f_vec3` | x, y, z | float3 from parts |
| `f_vec4` | x, y, z, w | float4 from parts |
| `f_ivec2` | x, y | integer2 from parts |
| `f_ivec3` | x, y, z | integer3 from parts |
| `f_ivec4` | x, y, z, w | integer4 from parts |
| `f_swizzle` | a | pick or reorder components: mask (xyzw or rgba, e.g. "zyx") |
| `f_neg` | a | negate |
| `f_abs` | a | absolute value |
| `f_sqrt` | a | square root (0 for negative input) |
| `f_exp` | a | e to the power a |
| `f_log` | a | natural logarithm (0 for input <= 0) |
| `f_log2` | a | base-2 logarithm (0 for input <= 0) |
| `f_sin` | a | sine (radians) |
| `f_cos` | a | cosine (radians) |
| `f_tan` | a | tangent (radians) |
| `f_asin` | a | inverse sine (input clamped to -1..1) |
| `f_acos` | a | inverse cosine (input clamped to -1..1) |
| `f_atan` | a | inverse tangent |
| `f_radians` | a | degrees to radians |
| `f_degrees` | a | radians to degrees |
| `f_floor` | a | round down |
| `f_ceil` | a | round up |
| `f_round` | a | round to nearest |
| `f_fract` | a | fractional part |
| `f_sign` | a | -1, 0 or 1 |
| `f_saturate` | a | clamp to 0..1 |
| `f_not` | a | boolean not |
| `f_length` | a | length of a vector |
| `f_normalize` | a | vector of length 1 (zero stays zero) |
| `f_add` | a, b | a + b |
| `f_sub` | a, b | a - b |
| `f_mul` | a, b | a * b |
| `f_div` | a, b | a / b (0 when b is 0; integer division truncates) |
| `f_mod` | a, b | remainder of a / b with the sign of b (0 when b is 0) |
| `f_pow` | a, b | a to the power b |
| `f_min` | a, b | smaller value |
| `f_max` | a, b | larger value |
| `f_atan2` | a, b | angle of the point (b, a) |
| `f_step` | a, b | 0 when b < a, else 1 |
| `f_dot` | a, b | dot product |
| `f_cross` | a, b | cross product of two float3 |
| `f_distance` | a, b | distance between two points |
| `f_clamp` | a, b, c | clamp a between b (low) and c (high) |
| `f_lerp` | a, b, c | a + (b - a) * c |
| `f_smoothstep` | a, b, c | smooth 0..1 ramp of c between edges a and b |
| `f_eq` | a, b | a == b |
| `f_ne` | a, b | a != b |
| `f_lt` | a, b | a < b |
| `f_le` | a, b | a <= b |
| `f_gt` | a, b | a > b |
| `f_ge` | a, b | a >= b |
| `f_and` | a, b | boolean and |
| `f_or` | a, b | boolean or |
| `f_xor` | a, b | boolean exclusive or |
| `f_if` | cond, a, b | a when cond is true, else b (only the chosen branch runs) |
| `f_sequence` | a, b | run a, then b; returns b |
| `f_set` | a | store a in the variable `name`; returns a |
| `f_get` |  | read the variable `name` (0 until it is set) |
| `f_random` | a, b | random float between a (default 0) and b (default 1); `seed` shifts the sequence |
| `f_sample_gray` | a | brightness of input image `slot` (default in) at the position a (float2, 0..1) |
| `f_sample_color` | a | colour (float4) of input image `slot` at the position a |
| `f_image_avg` |  | average colour (float4) of input image `slot` |
| `f_image_min` |  | smallest channel values (float4) of input image `slot` |
| `f_image_max` |  | largest channel values (float4) of input image `slot` |
| `f_pos` |  | position being evaluated (float2, 0..1) |
| `f_size` |  | output size in pixels (float2) |
| `f_index` |  | instance / iteration number (integer) |
| `f_count` |  | number of instances / iterations (integer) |
| `f_seed` |  | random seed of the caller (integer) |
| `f_input` |  | input of this function: name, type (0 float, 1 int, 2 bool, 3 float2, 4 float3, 5 float4, 6 int2, 7 int3, 8 int4, 9 string), default v / x y z w |
| `f_graph_input` |  | value of an Input Value node of the graph the caller lives in: name, type, default v / x y z w |
| `f_to_float` | a | convert to float |
| `f_to_int` | a | convert to integer (drops the fraction) |
| `f_to_bool` | a | non-zero is true |
| `f_to_string` | a | text of a value |
| `f_concat` | a, b | join two strings |
| `f_call` | a, b, c, d | run another function (`fn`); a b c d fill its inputs in the order of their node numbers |

## Pattern-program nodes

Used inside graphs with `meta kind "fxmap"`, run by the `fx_map` node. A cell (centre, size, rotation, value, colour) is passed down; quadrants and iterations change it, markers draw.

| Node | Inputs | Description and parameters |
|---|---|---|
| `fxm_out` | in | start of a pattern program |
| `fxm_marker` |  | draw a shape: shape (0 circle 1 square 2 diamond 3 image) src x y w h rot value r g b softness rnd_pos rnd_scale rnd_rot rnd_val rnd_color |
| `fxm_quadrant` | q0, q1, q2, q3 | split the cell in four: scale off_x off_y p0..p3 (chance) v0..v3 (value) r0..r3 (rotation) rnd_pos rnd_scale rnd_rot rnd_val |
| `fxm_iterate` | body | repeat body: count dx dy dscale drot dval, or grid_x grid_y stagger; rnd_x rnd_y rnd_scale rnd_rot rnd_val |
| `fxm_switch` | a, b, c, d | run one branch: mode (0 index, 1 random) index w0..w3 |

## Splines and path nodes

Every path node can merge up to four splines (`file`, `file2`, `file3`, `file4`) and place them with `path_scale`, `path_rotate` (turns), `path_x`, `path_y`, `path_mirror` (0 none, 1 horizontal, 2 vertical, 3 both) and `path_reverse`; `path` and `path_count` choose a range of paths. `path_sample` writes position, width, height and side maps, `path_warp` bends a picture near a spline, and `path_splatter` repeats a pattern in rows along it. A mask or any picture can be turned into a spline with `materialcraft trace` or the node menu entry Trace to spline.

A spline is a list of paths made of points with optional bezier handles and a width factor. It is stored in the package as an embedded text resource (usually `name.spl`, see `UXMAP.md` section 3 for the format) and a path node names it with `file="name.spl"`. Without a file the node uses a built-in shape (`preset`: 0 line, 1 diagonal, 2 wave, 3 circle, 4 spiral, 5 zigzag, 6 figure eight, with `amplitude` and `frequency`). `path` picks one path of the spline (-1 = all). Coordinates are texture coordinates, so a spline wraps with the texture and the result always tiles. Widths and distances are given in pixels for a 512 px texture and scale with the size. With `fit=1` dashes, sleepers, stitches and scattered copies are spaced so the pattern meets itself where a path wraps around the texture. The editor draws and edits splines in the 2D view; `Import SVG paths` turns the outlines of a drawing into a spline.

## Mesh baking

The mesh map baker (Tools > Mesh map baker, or `materialcraft bake`) turns a low-poly mesh with UVs, optionally with high-poly detail and a cage, into texture maps. Meshes can be OBJ, glTF / GLB (including external `.bin` buffers and data URIs), PLY (ASCII and binary) or STL (ASCII and binary). Several low and several high files can be baked together.

Maps: tangent-space normal, world-space normal, position, ambient occlusion, bent normal (tangent and world space), curvature, thickness, height from cage, object ID, material ID, polygroup ID, vertex colour, opacity (ray hits), world-space direction and UV layout. Settings: size (up to 16384 px, non-square allowed), anti-aliasing 1x1 to 8x8, padding, front and rear ray distance, cage mesh or averaged-normal cage, match low and high objects by name suffix (`_low` / `_high` and others), normal handling per mesh (file normals, flat, smooth by angle, smoothing groups), per-vertex or per-face tangent basis, OpenGL or DirectX normal maps, first or second UV set, a single material, back-face culling, and the ray counts and distances for AO, thickness and curvature. The baker runs on a background thread with progress and Cancel and reports coverage, ray statistics and warnings.

Results are stored in the package as 16-bit PNG resources named `bake/<set>/<map>.png` (and the set's settings as `bake/<set>.cfg`), so they travel with the file and publish with it. A `baked_map` node reads one map by set and map name and resamples it to the graph size; `mesh_weathering` and the older `edge_wear`, `dirt`, `dust`, `leak_streaks` and `settled_dust` nodes turn baked curvature, AO and normal maps into masks.

## Planned node families

Described by what they do; see `RELEASE_PLAN.md` for the order.

- More noises: simplex and ridged 3D noises, and more grunge and dirt recipes.
- More patterns: brick variants, shape splatter with data extraction.
- More adjustments: contrast / luminosity, linear / sRGB conversion, colour lookup tables, histogram select and equalise, frequency tools, pre-multiplied conversion.
- More effects: UV-aware diffusion variants, extend shape.
- More transforms: auto crop, polar conversion, clone, noise upscaling, safe and non-square transforms, trapezoid / quad transforms.
- Normal tools: bent and facing normals, height-normal blending, world-unit height to normal, normal transform.
- Material tools: PBR validators, layers, more palette tools.
- Path extras: quadratic and circle helpers, vertex processing.
- Splines and paths with shape, scatter and mapping nodes.
