-- SPDX-License-Identifier: GPL-3.0-or-later
-- Tests for the Lightroom Classic plug-in, outside Lightroom: Kit.lua on its
-- own, then the two menu commands against a stand-in for Lightroom's API that
-- behaves as the plug-in expects (it doesn't render; an export is an empty
-- file). It catches mistakes in the plug-in's own logic; whether Lightroom
-- agrees is what the 0-check run is for.
--
--   lua5.1 tools/lightroom-kit/test-plugin.lua [SESSION-DIR]
--
-- With a session folder (made by make-kit.sh or `amb sweep generate`), the
-- session test also runs on a copy of its manifest and sidecars.

local here = arg[0]:match("^(.*)[/\\]") or "."
local plugin = here .. "/ambrotype-kit.lrplugin"
package.path = plugin .. "/?.lua;" .. package.path
local Kit = require "Kit"

local failures = 0
local function check(cond, what)
  if not cond then
    failures = failures + 1
    print("FAIL: " .. what)
  end
end

local function readFile(path)
  local f = io.open(path, "rb")
  if not f then
    return nil
  end
  local s = f:read("*a")
  f:close()
  return s
end

local function writeFile(path, text)
  local f = assert(io.open(path, "wb"))
  f:write(text)
  f:close()
end

local function exists(path)
  local f = io.open(path, "rb")
  if f then
    f:close()
  end
  return f ~= nil
end

local function count(t)
  local n = 0
  for _ in pairs(t) do
    n = n + 1
  end
  return n
end

-- Kit.lua -------------------------------------------------------------------

do
  local rows = Kit.parseManifest("file,source,group,tag,value,extra\r\n" ..
    "a__baseline.ARW,/k/my, raws/a.ARW,baseline,,,\r\n" ..
    "a__VignetteMidpoint_p25.ARW,/k/my, raws/a.ARW,lens,VignetteMidpoint,25,VignetteAmount=100\n")
  check(#rows == 2, "two manifest rows")
  check(rows[1].source == "/k/my, raws/a.ARW", "a comma in the source path: " .. tostring(rows[1].source))
  check(rows[1].group == "baseline" and rows[1].tag == "" and rows[1].value == "", "the baseline row's empty fields")
  check(rows[2].tag == "VignetteMidpoint" and rows[2].value == "25" and rows[2].extra == "VignetteAmount=100", "a step row")

  check(Kit.value("+1") == 1 and Kit.value("-0.5") == -0.5, "signed numbers")
  check(Kit.value("True") == true and Kit.value("As Shot") == "As Shot", "booleans and text")

  local s = Kit.parseSidecar('<x crs:HasSettings="True"\n crs:Exposure2012="+1"\n crs:WhiteBalance="As Shot"/>')
  check(s.HasSettings == nil and s.Exposure2012 == 1 and s.WhiteBalance == "As Shot", "a sidecar's settings")

  local start = { Exposure2012 = 0, Contrast2012 = 0, WhiteBalance = "As Shot", Temperature = 5200, Tint = 4 }
  local touched = {}
  local t = Kit.stepSettings({ Exposure2012 = 1, WhiteBalance = "As Shot" }, touched, start)
  check(t.Exposure2012 == 1 and t.Contrast2012 == nil, "the first step is only its own settings")
  t = Kit.stepSettings({ Contrast2012 = 50, WhiteBalance = "As Shot" }, touched, start)
  check(t.Contrast2012 == 50 and t.Exposure2012 == 0, "an earlier step's setting goes back to its start")
  t = Kit.stepSettings({ Temperature = 6500, WhiteBalance = "Custom" }, touched, start)
  check(t.Temperature == 6500 and t.Contrast2012 == 0, "a custom white balance")
  t = Kit.stepSettings({ Exposure2012 = 2, WhiteBalance = "As Shot" }, touched, start)
  check(t.Temperature == nil and t.WhiteBalance == "As Shot", "As Shot sets Temperature itself")
  t = Kit.stepSettings({}, touched, start)
  check(t.WhiteBalance == "As Shot" and t.Temperature == nil and t.Exposure2012 == 0, "back to the start")

  local d = Kit.differences({ A = 1, B = 2, C = "x" }, { A = 1.00001, B = 3, C = "x" })
  check(#d == 1 and d[1][1] == "B", "differences beyond rounding")

  check(Kit.defaultFormat("2-real") == "tiff-1024" and Kit.defaultFormat("4-lens") == "jpeg-full"
    and Kit.defaultFormat("0-check") == "tiff-full", "each session's export preset")
  local e = Kit.exportSettings("tiff-1024", "/out", "a__x")
  check(e.LR_format == "TIFF" and e.LR_export_bitDepth == 16 and e.LR_size_maxWidth == 1024
    and e.LR_tokenCustomString == "a__x" and e.LR_export_destinationPathPrefix == "/out", "export settings")
  check(Kit.exportSettings("jpeg-full", "/o", "n").LR_export_colorSpace == "sRGB", "JPEG in sRGB")
  check(Kit.stem("a__Exposure2012_m0.5.ARW") == "a__Exposure2012_m0.5", "a file's stem")
end

-- A stand-in for Lightroom ----------------------------------------------------

local LR = {}
local mocks = {}
function import(name) -- luacheck: ignore (Lightroom's global)
  return assert(mocks[name], "no stand-in for " .. name)
end

-- A raw as Lightroom shows it right after import: its raw defaults.
local function rawDefaults()
  return { WhiteBalance = "As Shot", Temperature = 5200, Tint = 4, Exposure2012 = 0, Contrast2012 = 0,
    Sharpness = 40, ColorNoiseReduction = 25, LuminanceSmoothing = 0, LensProfileEnable = 1,
    AutoLateralCA = 0, PostCropVignetteAmount = 0, GrainAmount = 0, VignetteAmount = 0, VignetteMidpoint = 50 }
end

local function newPhoto(path)
  local photo = { path = path, settings = rawDefaults() }
  -- A DNG's own settings, as Lightroom reads them on import.
  if path:lower():find("%.dng$") then
    for k, v in pairs(Kit.parseSidecar(readFile(path) or "")) do
      photo.settings[k] = v
    end
  end
  function photo:applyDevelopSettings(s)
    assert(LR.writing, "applyDevelopSettings outside withWriteAccessDo")
    for k, v in pairs(s) do
      self.settings[k] = v
    end
    if s.WhiteBalance == "As Shot" then
      self.settings.Temperature, self.settings.Tint = 5200, 4
    elseif s.Temperature and not s.WhiteBalance then
      self.settings.WhiteBalance = "Custom"
    end
    LR.applies = LR.applies + 1
  end
  function photo:getDevelopSettings()
    local copy = {}
    for k, v in pairs(self.settings) do
      copy[k] = v
    end
    return copy
  end
  return photo
end

local function resetLightroom(dialogFolder)
  LR.photos, LR.exports, LR.messages, LR.applies, LR.writing = {}, {}, {}, 0, false
  LR.dialogFolder = dialogFolder
  LR.catalog = {}
  function LR.catalog:findPhotoByPath(path)
    return LR.photos[path]
  end
  function LR.catalog:addPhoto(path)
    assert(LR.writing, "addPhoto outside withWriteAccessDo")
    assert(exists(path), "addPhoto of a missing file " .. path)
    LR.photos[path] = newPhoto(path)
    return LR.photos[path]
  end
  function LR.catalog:withWriteAccessDo(_, fn)
    LR.writing = true
    fn()
    LR.writing = false
  end
  function LR.catalog:getTargetPhoto()
    return LR.target
  end
end

local factory = setmetatable({}, { __index = function() return function(_, t) return t end end })
mocks.LrView = { osFactory = function() return factory end, bind = function(k) return k end }
mocks.LrBinding = { makePropertyTable = function() return {} end }
mocks.LrFunctionContext = { postAsyncTaskWithContext = function(_, fn) fn({}) end }
mocks.LrTasks = { pcall = pcall, sleep = function() end }
mocks.LrApplication = { activeCatalog = function() return LR.catalog end, versionString = function() return "test" end }
mocks.LrApplicationView = { switchToModule = function() end }
mocks.LrDialogs = {
  runOpenPanel = function() return { LR.dialogFolder } end,
  runSavePanel = function() return LR.savePath end,
  presentModalDialog = function() return "ok" end,
  attachErrorDialogToFunctionContext = function() end,
  message = function(title, text) LR.messages[#LR.messages + 1] = title .. ": " .. tostring(text) end,
}
mocks.LrProgressScope = function()
  return { isCanceled = function() return false end, setCaption = function() end,
    setPortionComplete = function() end, done = function() end }
end
mocks.LrPathUtils = {
  child = function(a, b) return a .. "/" .. b end,
  parent = function(p) return p:match("^(.*)/[^/]*$") end,
  leafName = function(p) return p:match("([^/]*)$") end,
}
mocks.LrFileUtils = {
  exists = exists,
  createAllDirectories = function(p) os.execute("mkdir -p '" .. p .. "'") end,
}
mocks.LrExportSession = function(t)
  return {
    doExportOnCurrentTask = function()
      local s = t.exportSettings
      local photo = t.photosToExport[1]
      local ext = s.LR_format == "JPEG" and "jpg" or "tif"
      writeFile(s.LR_export_destinationPathPrefix .. "/" .. s.LR_tokenCustomString .. "." .. ext, "")
      LR.exports[s.LR_tokenCustomString] = photo:getDevelopSettings()
    end,
  }
end

local tmp = (os.getenv("TMPDIR") or "/tmp") .. "/amb-lrplugin-test-" .. tostring(os.time()) .. "-" .. tostring(math.random(1e6))
os.execute("rm -rf '" .. tmp .. "' && mkdir -p '" .. tmp .. "'")

-- A session folder: manifest.csv, the raws' links (empty files here), their
-- sidecars and the synthetic DNGs.
local function sidecar(settings)
  local lines = { '<x:xmpmeta xmlns:x="adobe:ns:meta/">', ' <rdf:Description', '     crs:HasSettings="True"' }
  local names = {}
  for k in pairs(settings) do
    names[#names + 1] = k
  end
  table.sort(names)
  for _, k in ipairs(names) do
    lines[#lines + 1] = string.format('     crs:%s="%s"', k, tostring(settings[k]))
  end
  return table.concat(lines, "\n") .. "/>\n</x:xmpmeta>\n"
end

local function makeSession(dir)
  os.execute("mkdir -p '" .. dir .. "'")
  local base = { WhiteBalance = "As Shot", Sharpness = 0, LuminanceSmoothing = 0, ColorNoiseReduction = 0,
    LensProfileEnable = 0, AutoLateralCA = 0, PostCropVignetteAmount = 0, GrainAmount = 0 }
  local lines = { "file,source,group,tag,value,extra" }
  local function step(stem, ext, source, group, tag, value, extra)
    local s = {}
    for k, v in pairs(base) do
      s[k] = v
    end
    for k, v in pairs(extra or {}) do
      s[k] = v
    end
    if tag ~= "" then
      s[tag] = value
    end
    local ex = {}
    for k, v in pairs(extra or {}) do
      if k ~= "WhiteBalance" then
        ex[#ex + 1] = k .. "=" .. tostring(v)
      end
    end
    lines[#lines + 1] = table.concat({ stem .. "." .. ext, source, group, tag, value and tostring(value) or "", table.concat(ex, ";") }, ",")
    if source:find("^synthetic:") then
      writeFile(dir .. "/" .. stem .. "." .. ext, "DNG bytes\0" .. sidecar(s))
    else
      writeFile(dir .. "/" .. stem .. "." .. ext, "")
      writeFile(dir .. "/" .. stem .. ".xmp", sidecar(s))
    end
  end
  step("2414__baseline", "ARW", "/k/raws/2414.ARW", "baseline", "")
  step("2414__Exposure2012_p1", "ARW", "/k/raws/2414.ARW", "tone", "Exposure2012", 1)
  step("2414__Temperature_p6500", "ARW", "/k/raws/2414.ARW", "white_balance", "Temperature", 6500, { WhiteBalance = "Custom" })
  step("2414__Contrast2012_p50", "ARW", "/k/raws/2414.ARW", "tone", "Contrast2012", 50)
  step("2414__VignetteMidpoint_p25", "ARW", "/k/raws/2414.ARW", "lens", "VignetteMidpoint", 25, { VignetteAmount = 100 })
  step("2414__Sharpness_p50", "ARW", "/k/raws/2414.ARW", "detail", "Sharpness", 50)
  step("983__baseline", "CR2", "/k/my, raws/983.CR2", "baseline", "")
  step("983__Exposure2012_m1", "CR2", "/k/my, raws/983.CR2", "tone", "Exposure2012", -1)
  step("ramp__baseline", "dng", "synthetic:ramp", "baseline", "")
  step("ramp__Exposure2012_p1", "dng", "synthetic:ramp", "tone", "Exposure2012", 1)
  writeFile(dir .. "/manifest.csv", table.concat(lines, "\n") .. "\n")
end

local function runSession(dir, label)
  local name = dir:match("([^/]*)$")
  local out = dir:match("^(.*)/[^/]*$") .. "/lr-out/" .. name
  local rows = Kit.parseManifest(readFile(dir .. "/manifest.csv"))
  resetLightroom(dir)
  dofile(plugin .. "/RunSession.lua")
  local exports, photos = LR.exports, LR.photos
  local made = 0
  for _, row in ipairs(rows) do
    local stem = Kit.stem(row.file)
    local ext = Kit.defaultFormat(name) == "jpeg-full" and "jpg" or "tif"
    if exists(out .. "/" .. stem .. "." .. ext) then
      made = made + 1
    end
    -- Each export holds its step's value, and nothing an earlier step set.
    local got = LR.exports[stem]
    if got and row.tag ~= "" then
      check(got[row.tag] == Kit.value(row.value), label .. ": " .. stem .. " exported with " .. tostring(got[row.tag]))
    end
  end
  check(made == #rows, label .. ": " .. made .. " of " .. #rows .. " exported")
  check(count(LR.exports) == #rows, label .. ": one export per row")
  local log = readFile(out .. "/kit-log.csv") or ""
  local logged = 0
  for line in log:gmatch("[^\n]+") do
    if line:find(",ok,") then
      logged = logged + 1
    end
  end
  check(logged == #rows, label .. ": " .. logged .. " ok lines in the log\n" .. log)
  check(#LR.messages == 1 and LR.messages[1]:find(#rows .. " exported, 0 already there, 0 with a problem", 1, true),
    label .. ": the closing message: " .. tostring(LR.messages[1]))

  -- Running it again skips what's there.
  resetLightroom(dir)
  dofile(plugin .. "/RunSession.lua")
  check(count(LR.exports) == 0, label .. ": nothing exported twice")
  check(LR.messages[1] and LR.messages[1]:find("0 exported, " .. #rows .. " already there", 1, true),
    label .. ": the second run's message: " .. tostring(LR.messages[1]))
  return exports, photos
end

do
  local dir = tmp .. "/kit/2-real"
  makeSession(dir)
  local e, photos = runSession(dir, "made-up session")
  check(e["2414__Exposure2012_p1"].Sharpness == 0, "the baseline's Sharpness 0, not the raw default")
  check(e["2414__Contrast2012_p50"].Exposure2012 == 0, "Exposure back to 0 after its step")
  check(e["2414__Contrast2012_p50"].WhiteBalance == "As Shot" and e["2414__Contrast2012_p50"].Temperature == 5200,
    "white balance back to As Shot after the Temperature step")
  check(e["2414__Temperature_p6500"].WhiteBalance == "Custom", "the Temperature step is Custom")
  check(e["2414__VignetteMidpoint_p25"].VignetteAmount == 100, "a step's extra setting")
  check(e["2414__Sharpness_p50"].VignetteAmount == 0 and e["2414__Sharpness_p50"].VignetteMidpoint == 50,
    "the extra setting and the swept one back to their start")
  check(e["983__Exposure2012_m1"].Exposure2012 == -1, "the second raw, from a path with a comma")
  check(e["ramp__Exposure2012_p1"].Exposure2012 == 1, "a synthetic DNG's own settings")
  local raw = photos[dir .. "/2414__baseline.ARW"]
  check(raw and raw.settings.Exposure2012 == 0 and raw.settings.Contrast2012 == 0 and raw.settings.Sharpness == 0,
    "the raw is left at its baseline")
  check(photos[dir .. "/2414__Exposure2012_p1.ARW"] == nil, "only the baseline file of a raw is added")
end

if arg[1] then
  local copy = tmp .. "/given/" .. arg[1]:gsub("/+$", ""):match("([^/]*)$")
  os.execute("mkdir -p '" .. copy .. "'")
  local rows = Kit.parseManifest(assert(readFile(arg[1] .. "/manifest.csv"), "no manifest.csv in " .. arg[1]))
  writeFile(copy .. "/manifest.csv", readFile(arg[1] .. "/manifest.csv"))
  for _, row in ipairs(rows) do
    local stem = Kit.stem(row.file)
    -- A DNG's settings are inside it; a raw's contents don't matter here.
    local dng = row.file:lower():find("%.dng$")
    writeFile(copy .. "/" .. row.file, dng and readFile(arg[1] .. "/" .. row.file) or "")
    local xmp = readFile(arg[1] .. "/" .. stem .. ".xmp")
    if xmp then
      writeFile(copy .. "/" .. stem .. ".xmp", xmp)
    end
  end
  runSession(copy, arg[1])
end

-- Read the Develop sliders ----------------------------------------------------

do
  resetLightroom(tmp)
  local photo = newPhoto(tmp .. "/x.ARW")
  photo.settings.Exposure2012 = 0.5
  LR.target = photo
  LR.savePath = tmp .. "/sliders.csv"
  local ranges = { Exposure2012 = { -5, 5, 0, 0.01 }, Contrast2012 = { -100, 100, 0, 1 }, Temperature = { 2000, 50000, 5200, 50 } }
  mocks.LrDevelopController = {
    getRange = function(n) if ranges[n] then return ranges[n][1], ranges[n][2] end end,
    getValue = function(n) return photo.settings[n] end,
    setValue = function(n, v) photo.settings[n] = v end,
    resetToDefault = function(n) photo.settings[n] = ranges[n][3] end,
    increment = function(n) photo.settings[n] = photo.settings[n] + ranges[n][4] end,
    getProcessVersion = function() return "6.7" end,
  }
  dofile(plugin .. "/ReadSliders.lua")
  local csv = readFile(LR.savePath) or ""
  check(csv:find("^# Lightroom test, process version 6.7\n"), "the table's first line")
  check(csv:find("\nExposure2012,-5,5,0,0.01,0.5\n", 1, true), "Exposure's row:\n" .. csv)
  check(csv:find("\nTemperature,2000,50000,5200,50,5200\n", 1, true), "Temperature's row")
  check(not csv:find("Sharpness"), "a setting without a range is left out")
  check(photo.settings.Exposure2012 == 0.5, "each slider put back")
end

os.execute("rm -rf '" .. tmp .. "'")
if failures > 0 then
  print(failures .. " failed")
  os.exit(1)
end
print("lightroom plug-in: all passed")
