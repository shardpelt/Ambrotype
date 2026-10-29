-- SPDX-License-Identifier: GPL-3.0-or-later
-- What the plug-in does that needs no Lightroom: reading a kit folder's
-- manifest and sidecars, choosing what to apply for each step, and the
-- export settings. Plain Lua 5.1, so tools/lightroom-kit/test-plugin.lua can
-- test it outside Lightroom.

local Kit = {}

-- The rows of a kit's manifest.csv (`amb sweep generate`): file, source,
-- group, tag, value, extra. Fields aren't quoted; only `source`, a path,
-- could hold a comma, so it takes whatever is left in the middle.
function Kit.parseManifest(text)
  local rows = {}
  local first = true
  for line in (text .. "\n"):gmatch("([^\n]*)\n") do
    line = line:gsub("\r$", "")
    if first then
      first = false
    elseif line ~= "" then
      local f = {}
      for field in (line .. ","):gmatch("([^,]*),") do
        f[#f + 1] = field
      end
      if #f >= 6 then
        local n = #f
        rows[#rows + 1] = {
          file = f[1],
          source = table.concat(f, ",", 2, n - 4),
          group = f[n - 3],
          tag = f[n - 2],
          value = f[n - 1],
          extra = f[n],
        }
      end
    end
  end
  return rows
end

-- A setting's value as Lightroom takes it: a number, true/false, or text.
function Kit.value(text)
  if text == "True" then
    return true
  elseif text == "False" then
    return false
  end
  return tonumber(text) or text
end

-- The develop settings in a kit sidecar: `crs:Name="value"` attributes.
-- HasSettings only marks the sidecar as having some, so it's left out.
function Kit.parseSidecar(text)
  local settings = {}
  for name, value in text:gmatch('crs:([%w_]+)="([^"]*)"') do
    if name ~= "HasSettings" then
      settings[name] = Kit.value(value)
    end
  end
  return settings
end

-- The table to apply for one step, so the photo ends up as if it had been
-- imported with that step's sidecar: the step's own settings, plus the
-- starting value of anything an earlier step changed and this one doesn't
-- set. `touched` collects what has been changed so far. Temperature and Tint
-- are left to the white balance unless it is Custom (As Shot sets them).
function Kit.stepSettings(settings, touched, start)
  local t = {}
  for k, v in pairs(settings) do
    t[k] = v
  end
  for k in pairs(touched) do
    if t[k] == nil and start[k] ~= nil and k ~= "Temperature" and k ~= "Tint" then
      t[k] = start[k]
    end
  end
  if t.WhiteBalance == nil or t.WhiteBalance == "Custom" then
    for _, k in ipairs { "Temperature", "Tint" } do
      if t[k] == nil and touched[k] and start[k] ~= nil then
        t[k] = start[k]
      end
    end
  end
  for k in pairs(t) do
    touched[k] = true
  end
  return t
end

local function same(a, b)
  if type(a) == "number" and type(b) == "number" then
    return math.abs(a - b) <= 1e-4 * math.max(1, math.abs(a))
  end
  return a == b
end

-- Settings Lightroom holds differently from what was asked, sorted by name:
-- { name, wanted, got } each.
function Kit.differences(wanted, got)
  local out = {}
  for k, v in pairs(wanted) do
    if not same(v, got[k]) then
      out[#out + 1] = { k, v, got[k] }
    end
  end
  table.sort(out, function(a, b) return a[1] < b[1] end)
  return out
end

-- The three export presets of the trial plan.
Kit.FORMATS = {
  { id = "tiff-full", ext = "tif", title = "amb full TIFF16: TIFF, ProPhoto RGB, 16 bits, full size" },
  { id = "tiff-1024", ext = "tif", title = "amb 1024 TIFF16: the same, long edge 1024 px" },
  { id = "jpeg-full", ext = "jpg", title = "amb full JPEG: JPEG 95, sRGB, full size" },
}

function Kit.format(id)
  for _, f in ipairs(Kit.FORMATS) do
    if f.id == id then
      return f
    end
  end
  return Kit.FORMATS[1]
end

-- The format the trial plan uses for a session folder (2-real, 4-lens, …).
function Kit.defaultFormat(session)
  if session:find("^2%-") then
    return "tiff-1024"
  elseif session:find("^4%-") then
    return "jpeg-full"
  end
  return "tiff-full"
end

-- Export settings for one file named `name` (no extension) in `folder`.
-- The keys are the ones Lightroom writes in its own export presets; a
-- 0-check run confirms them against an export made by hand.
function Kit.exportSettings(id, folder, name)
  local s = {
    LR_export_destinationType = "specificFolder",
    LR_export_destinationPathPrefix = folder,
    LR_export_useSubfolder = false,
    LR_collisionHandling = "overwrite",
    LR_renamingTokensOn = true,
    LR_tokens = "{{custom_token}}",
    LR_tokenCustomString = name,
    LR_extensionCase = "lowercase",
    LR_outputSharpeningOn = false,
    LR_minimizeEmbeddedMetadata = false,
    LR_embeddedMetadataOption = "all",
    LR_removeLocationMetadata = false,
    LR_useWatermark = false,
    LR_reimportExportedPhoto = false,
    LR_size_doConstrain = false,
  }
  if id == "jpeg-full" then
    s.LR_format = "JPEG"
    s.LR_jpeg_quality = 0.95
    s.LR_jpeg_useLimitSize = false
    s.LR_export_colorSpace = "sRGB"
    s.LR_export_bitDepth = 8
  else
    s.LR_format = "TIFF"
    s.LR_tiff_compressionMethod = "compressionMethod_ZIP"
    s.LR_export_colorSpace = "ProPhotoRGB"
    s.LR_export_bitDepth = 16
  end
  if id == "tiff-1024" then
    s.LR_size_doConstrain = true
    s.LR_size_resizeType = "longEdge"
    s.LR_size_maxWidth = 1024
    s.LR_size_maxHeight = 1024
    s.LR_size_units = "pixels"
    s.LR_size_doNotEnlarge = true
  end
  return s
end

-- The file name without its extension.
function Kit.stem(file)
  return (file:gsub("%.[^./\\]*$", ""))
end

-- One line of the log: fields joined by commas (commas inside become ;).
function Kit.logLine(fields)
  local out = {}
  for i, f in ipairs(fields) do
    out[i] = tostring(f):gsub("[,\r\n]", ";")
  end
  return table.concat(out, ",") .. "\n"
end

-- The differences as one field: Name wanted→got; …
function Kit.describe(diffs)
  local out = {}
  for i, d in ipairs(diffs) do
    out[i] = string.format("%s %s→%s", d[1], tostring(d[2]), tostring(d[3]))
  end
  return table.concat(out, "; ")
end

return Kit
