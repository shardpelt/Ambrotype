# Ambrotype core card
Edit: `amb do PHOTO "exp=+0.5 con+=10 reset:hsl"` [--dry-run]; MCP edit_photo {photo, do}. One undo step; prints net changes.
Items: Name=v Name+=d Name-=d reset:Name|group preset:PATH[@amount] sky:|subject:|background:|people:Name=v,… (a new mask) snap:N load:N undo redo
Check: `amb stats PHOTO` (L*, clipping, cast) before a picture (`amb render PHOTO out.jpg --long-edge 800`).
Match a look: `amb match PHOTO REF.jpg...` (shows changes; --apply).
More: `amb ref GROUP`: metadata process profile white_balance tone tone_legacy presence tone_curve hsl bw_mixer point_color color_grading split_toning detail lens transform effects lut calibration crop local retouch enhance lens_blur hdr negative
Short name, setting, range =default:
temp Temperature 2000..50000 K (as shot)
tint Tint -150..150
exp Exposure2012 -5..5 =0
con Contrast2012 -100..100 =0
hl Highlights2012 -100..100 =0
sh Shadows2012 -100..100 =0
wh Whites2012 -100..100 =0
bl Blacks2012 -100..100 =0
tex Texture -100..100 =0
cla Clarity2012 -100..100 =0
deh Dehaze -100..100 =0
vib Vibrance -100..100 =0
sat Saturation -100..100 =0
sharp Sharpness 0..150 =40
nr LuminanceSmoothing 0..100 =0
cnr ColorNoiseReduction 0..100 =25
vig PostCropVignetteAmount -100..100 =0
grain GrainAmount 0..100 =0
HSL: {Hue,Saturation,Luminance}Adjustment{Red,Orange,Yellow,Green,Aqua,Blue,Purple,Magenta} -100..100
Grading: ColorGrade{Shadow,Midtone,Highlight,Global}{Hue 0..359,Sat 0..100,Lum -100..100}
B&W: ConvertToGrayscale=true
