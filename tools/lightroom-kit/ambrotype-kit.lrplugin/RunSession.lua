-- SPDX-License-Identifier: GPL-3.0-or-later
-- File → Plug-in Extras → Run a kit session…
--
-- Pick a session folder of the kit (it has a manifest.csv). Each raw in it is
-- added to the catalogue once (its __baseline file); then, row by row, the
-- step's sidecar settings are applied to that one photo and it is exported
-- under the step's name. So a session of thousands of steps needs ten photos
-- in the catalogue, not thousands. Synthetic DNGs carry their settings
-- inside, so they are added and exported as they are.
--
-- Exports already there are skipped, so a stopped run carries on where it
-- was. Every step goes into kit-log.csv in the export folder, with any
-- setting Lightroom holds differently from what the sidecar asked.

local LrApplication = import "LrApplication"
local LrBinding = import "LrBinding"
local LrDialogs = import "LrDialogs"
local LrExportSession = import "LrExportSession"
local LrFileUtils = import "LrFileUtils"
local LrFunctionContext = import "LrFunctionContext"
local LrPathUtils = import "LrPathUtils"
local LrProgressScope = import "LrProgressScope"
local LrTasks = import "LrTasks"
local LrView = import "LrView"

local Kit = require "Kit"

local function readFile(path)
  local f = io.open(path, "rb")
  if not f then
    return nil
  end
  local text = f:read("*a")
  f:close()
  return text
end

-- The session folder, its rows, and where and how to export; nil if cancelled.
local function ask(context)
  local picked = LrDialogs.runOpenPanel {
    title = "Choose a kit session folder (the one with manifest.csv)",
    canChooseFiles = false,
    canChooseDirectories = true,
    allowsMultipleSelection = false,
  }
  if not picked or not picked[1] then
    return nil
  end
  local folder = picked[1]
  local manifest = readFile(LrPathUtils.child(folder, "manifest.csv"))
  if not manifest then
    LrDialogs.message("No manifest.csv here", folder .. " isn't a kit session folder: choose one made by make-kit.sh.", "critical")
    return nil
  end
  local rows = Kit.parseManifest(manifest)
  local name = LrPathUtils.leafName(folder)

  local f = LrView.osFactory()
  local props = LrBinding.makePropertyTable(context)
  props.format = Kit.defaultFormat(name)
  props.out = LrPathUtils.child(LrPathUtils.child(LrPathUtils.parent(folder), "lr-out"), name)
  local items = {}
  for i, fmt in ipairs(Kit.FORMATS) do
    items[i] = { title = fmt.title, value = fmt.id }
  end
  local result = LrDialogs.presentModalDialog {
    title = "Ambrotype kit: " .. name,
    actionVerb = "Run",
    contents = f:column {
      bind_to_object = props,
      spacing = f:control_spacing(),
      f:static_text { title = string.format("%d steps in %s", #rows, folder) },
      f:row {
        f:static_text { title = "Export as:", width = 80 },
        f:popup_menu { value = LrView.bind "format", items = items },
      },
      f:row {
        f:static_text { title = "Into:", width = 80 },
        f:edit_field { value = LrView.bind "out", width_in_chars = 50 },
      },
      f:static_text { title = "It can take hours. Cancel in the progress bar (top left) stops it; running it again carries on." },
    },
  }
  if result ~= "ok" then
    return nil
  end
  return { folder = folder, name = name, rows = rows, format = Kit.format(props.format), out = props.out }
end

local function photoAt(catalog, path)
  local photo = catalog:findPhotoByPath(path)
  if photo then
    return photo
  end
  catalog:withWriteAccessDo("Add kit photo", function()
    photo = catalog:addPhoto(path)
  end, { timeout = 60 })
  return photo
end

local function apply(catalog, photo, settings)
  catalog:withWriteAccessDo("Apply kit step", function()
    photo:applyDevelopSettings(settings, "Ambrotype kit")
  end, { timeout = 60 })
end

local function export(photo, job, name)
  local session = LrExportSession {
    photosToExport = { photo },
    exportSettings = Kit.exportSettings(job.format.id, job.out, name),
  }
  session:doExportOnCurrentTask()
end

local function run(context, job)
  local catalog = LrApplication.activeCatalog()
  LrFileUtils.createAllDirectories(job.out)
  local logPath = LrPathUtils.child(job.out, "kit-log.csv")
  local fresh = not LrFileUtils.exists(logPath)
  local log = assert(io.open(logPath, "a"))
  if fresh then
    log:write(Kit.logLine { "file", "result", "differences" })
  end

  -- Each raw's photo: its __baseline file, as Lightroom holds it after the
  -- baseline settings (the values later steps are reset to).
  local baselineOf = {}
  for _, row in ipairs(job.rows) do
    if row.group == "baseline" and not row.source:find("^synthetic:") then
      baselineOf[row.source] = row.file
    end
  end
  local raws = {}
  local function raw(source)
    if raws[source] then
      return raws[source]
    end
    local file = assert(baselineOf[source], "no __baseline row for " .. source)
    local photo = assert(photoAt(catalog, LrPathUtils.child(job.folder, file)), "couldn't add " .. file)
    local base = Kit.parseSidecar(assert(readFile(LrPathUtils.child(job.folder, Kit.stem(file) .. ".xmp"))))
    apply(catalog, photo, base)
    local r = { photo = photo, start = photo:getDevelopSettings(), touched = {} }
    for k in pairs(base) do
      r.touched[k] = true
    end
    raws[source] = r
    return r
  end

  local function step(row)
    local name = Kit.stem(row.file)
    local target = LrPathUtils.child(job.out, name .. "." .. job.format.ext)
    if LrFileUtils.exists(target) then
      return "skipped"
    end
    local photo, wanted
    if row.source:find("^synthetic:") then
      photo = assert(photoAt(catalog, LrPathUtils.child(job.folder, row.file)), "couldn't add " .. row.file)
      wanted = {}
      if row.tag ~= "" then
        wanted[row.tag] = Kit.value(row.value)
      end
    else
      local r = raw(row.source)
      local sidecar = assert(readFile(LrPathUtils.child(job.folder, name .. ".xmp")), "no sidecar for " .. row.file)
      wanted = Kit.stepSettings(Kit.parseSidecar(sidecar), r.touched, r.start)
      photo = r.photo
      apply(catalog, photo, wanted)
    end
    local diffs = Kit.differences(wanted, photo:getDevelopSettings())
    export(photo, job, name)
    if not LrFileUtils.exists(target) then
      return "no file", Kit.describe(diffs)
    end
    return #diffs == 0 and "ok" or "settings differ", Kit.describe(diffs)
  end

  local progress = LrProgressScope { title = "Ambrotype kit: " .. job.name, functionContext = context }
  local counts = { ok = 0, skipped = 0, problems = 0 }
  local failedInARow = 0
  for i, row in ipairs(job.rows) do
    if progress:isCanceled() then
      break
    end
    progress:setCaption(row.file)
    progress:setPortionComplete(i - 1, #job.rows)
    local ok, result, detail = LrTasks.pcall(step, row)
    if not ok then
      result, detail = "failed", tostring(result)
    end
    if result ~= "skipped" then
      log:write(Kit.logLine { row.file, result, detail or "" })
      log:flush()
    end
    if result == "ok" or result == "skipped" then
      counts[result] = counts[result] + 1
    else
      counts.problems = counts.problems + 1
    end
    failedInARow = result == "failed" and failedInARow + 1 or 0
    if failedInARow >= 5 then
      LrDialogs.message("Stopped after five failures in a row", tostring(detail), "critical")
      break
    end
  end
  progress:done()

  -- Leave each raw as its baseline, as it was imported.
  for _, r in pairs(raws) do
    LrTasks.pcall(apply, catalog, r.photo, Kit.stepSettings({}, r.touched, r.start))
  end
  log:close()
  LrDialogs.message(
    "Kit session " .. job.name .. " done",
    string.format("%d exported, %d already there, %d with a problem.\nLog: %s", counts.ok, counts.skipped, counts.problems, logPath),
    counts.problems > 0 and "warning" or "info")
end

LrFunctionContext.postAsyncTaskWithContext("Ambrotype kit session", function(context)
  LrDialogs.attachErrorDialogToFunctionContext(context)
  local job = ask(context)
  if job then
    run(context, job)
  end
end)
