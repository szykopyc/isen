-- Isen's extremely serious Neovim integration.
vim.filetype.add({
  extension = {
    is = "isen",
  },
})

if vim.g.isen_diagnostics == false then
  return
end

local namespace = vim.api.nvim_create_namespace("isen")
local generations = {}
local timers = {}
local processes = {}
local last_failure = nil

local DEBOUNCE_MS = 200

local function report_failure(message)
  if message ~= last_failure then
    last_failure = message
    vim.notify(message, vim.log.levels.WARN, { title = "Isen diagnostics" })
  end
end

local function executable(path)
  if vim.g.isen_executable and vim.g.isen_executable ~= "" then
    return vim.g.isen_executable
  end
  if vim.fs and vim.fs.find then
    local launcher = vim.fs.find("isen", {
      upward = true,
      type = "file",
      path = vim.fs.dirname(path),
    })[1]
    if launcher then
      return launcher
    end
  end
  local installed = vim.fn.exepath("isen")
  return installed ~= "" and installed or "isen"
end

local function publish(buffer, generation, result)
  if not vim.api.nvim_buf_is_valid(buffer) or generations[buffer] ~= generation then
    return
  end
  if result.code ~= 0 and result.code ~= 1 then
    vim.diagnostic.reset(namespace, buffer)
    local failure = vim.trim(result.stderr or "")
    report_failure(failure ~= "" and failure or "Isen diagnostics process failed")
    return
  end
  local ok, report = pcall(vim.json.decode, result.stdout or "")
  if not ok or report.format ~= "isen-diagnostics-v1" then
    vim.diagnostic.reset(namespace, buffer)
    report_failure("Isen returned an unsupported diagnostics document")
    return
  end
  last_failure = nil
  local diagnostics = {}
  local buffer_name = vim.api.nvim_buf_get_name(buffer)
  local buffer_path = vim.fs.normalize(vim.uv.fs_realpath(buffer_name) or buffer_name)
  for _, diagnostic in ipairs(report.diagnostics or {}) do
    if vim.fs.normalize(diagnostic.path) == buffer_path then
      table.insert(diagnostics, {
        lnum = math.max((diagnostic.line or 1) - 1, 0),
        col = math.max((diagnostic.column or 1) - 1, 0),
        end_lnum = math.max((diagnostic.end_line or diagnostic.line or 1) - 1, 0),
        end_col = math.max((diagnostic.end_column or diagnostic.column or 1) - 1, 0),
        severity = vim.diagnostic.severity.ERROR,
        source = "isen",
        message = diagnostic.message or "unknown Isen error",
      })
    end
  end
  vim.diagnostic.set(namespace, buffer, diagnostics, {})
end

local function stop_timer(buffer)
  local timer = timers[buffer]
  if timer then
    timer:stop()
    timer:close()
    timers[buffer] = nil
  end
end

local function stop_process(buffer)
  local process = processes[buffer]
  if process then
    processes[buffer] = nil
    process:kill()
  end
end

local function start_process(buffer, generation, path, program)
  local process
  process = vim.system({ program, "--diagnostics", path }, { text = true }, function(result)
    if processes[buffer] == process then
      processes[buffer] = nil
    end
    vim.schedule(function()
      publish(buffer, generation, result)
    end)
  end)
  processes[buffer] = process
end

local function check(buffer)
  buffer = buffer or vim.api.nvim_get_current_buf()
  local path = vim.api.nvim_buf_get_name(buffer)
  if path == "" or vim.bo[buffer].filetype ~= "isen" then
    return
  end
  local program = executable(path)
  generations[buffer] = (generations[buffer] or 0) + 1
  local generation = generations[buffer]

  stop_timer(buffer)
  stop_process(buffer)

  local timer = vim.uv.new_timer()
  timers[buffer] = timer
  timer:start(DEBOUNCE_MS, 0, function()
    timers[buffer] = nil
    timer:close()
    start_process(buffer, generation, path, program)
  end)
end

local group = vim.api.nvim_create_augroup("IsenDiagnostics", { clear = true })
vim.api.nvim_create_autocmd({ "BufReadPost", "BufWritePost" }, {
  group = group,
  pattern = "*.is",
  callback = function(event)
    check(event.buf)
  end,
})
vim.api.nvim_create_autocmd("BufDelete", {
  group = group,
  pattern = "*.is",
  callback = function(event)
    stop_timer(event.buf)
    stop_process(event.buf)
    generations[event.buf] = nil
    vim.diagnostic.reset(namespace, event.buf)
  end,
})
vim.api.nvim_create_user_command("IsenDiagnostics", function()
  check()
end, {})
