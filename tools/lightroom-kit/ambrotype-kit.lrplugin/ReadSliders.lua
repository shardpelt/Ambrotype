-- SPDX-License-Identifier: GPL-3.0-or-later
-- File → Plug-in Extras → Read the Develop sliders…
--
-- For the photo open in Develop: every develop setting's lowest and highest
-- value, its default, and how far one step (an arrow key on the slider)
-- moves it, into a CSV file. This is part 4.1 of the trial plan without the
-- hand work. It moves each slider and puts it back, so use a scratch photo.

local LrApplication = import "LrApplication"
local LrApplicationView = import "LrApplicationView"
local LrDevelopController = import "LrDevelopController"
local LrDialogs = import "LrDialogs"
local LrFunctionContext = import "LrFunctionContext"
local LrProgressScope = import "LrProgressScope"
local LrTasks = import "LrTasks"

local Kit = require "Kit"

local function read(context)
  local photo = LrApplication.activeCatalog():getTargetPhoto()
  if not photo then
    LrDialogs.message("Select a photo first", "Select one of the kit's raws (a scratch copy is best), then run this again.", "warning")
    return
  end
  local path = LrDialogs.runSavePanel {
    title = "Save the slider table as",
    requiredFileType = "csv",
  }
  if not path then
    return
  end
  LrApplicationView.switchToModule("develop")
  LrTasks.sleep(1)

  local names = {}
  for name, value in pairs(photo:getDevelopSettings()) do
    if type(value) == "number" then
      names[#names + 1] = name
    end
  end
  table.sort(names)

  -- One row, or nil for a setting that isn't a slider.
  local function slider(name)
    local low, high = LrDevelopController.getRange(name)
    if low == nil then
      return nil
    end
    local was = LrDevelopController.getValue(name)
    LrDevelopController.resetToDefault(name)
    local default = LrDevelopController.getValue(name)
    LrDevelopController.increment(name)
    local step = LrDevelopController.getValue(name) - default
    LrDevelopController.setValue(name, was)
    return { name, low, high, default, step, was }
  end

  local _, pv = LrTasks.pcall(LrDevelopController.getProcessVersion)
  local out = assert(io.open(path, "w"))
  out:write("# Lightroom " .. tostring(LrApplication.versionString()) .. ", process version " .. tostring(pv) .. "\n")
  out:write(Kit.logLine { "name", "min", "max", "default", "step", "was" })
  local progress = LrProgressScope { title = "Reading the Develop sliders", functionContext = context }
  for i, name in ipairs(names) do
    if progress:isCanceled() then
      break
    end
    progress:setPortionComplete(i - 1, #names)
    local ok, row = LrTasks.pcall(slider, name)
    if ok and row then
      out:write(Kit.logLine(row))
    end
  end
  progress:done()
  out:close()
  LrDialogs.message("Slider table saved", path, "info")
end

LrFunctionContext.postAsyncTaskWithContext("Ambrotype: read the Develop sliders", function(context)
  LrDialogs.attachErrorDialogToFunctionContext(context)
  read(context)
end)
