-- The server's playtest stats (frame rate, memory, network, players),
-- queued once a second while the probe runs. Required by the server probe.
local Players = game:GetService("Players")
local RunService = game:GetService("RunService")
local Stats = game:GetService("Stats")

local M = {}

function M.start(queue, isRunning)
	local beats = 0
	RunService.Heartbeat:Connect(function()
		beats += 1
	end)

	task.spawn(function()
		while isRunning() do
			local started = os.clock()
			beats = 0
			task.wait(1)
			-- task.wait resumes a little late; rate over the real interval.
			local m = { fps = beats / math.max(os.clock() - started, 0.001) }
			local function read(key, f)
				local okRead, value = pcall(f)
				if okRead and type(value) == "number" then
					m[key] = value
				end
			end
			read("physics_fps", function()
				return workspace:GetRealPhysicsFPS()
			end)
			read("memory_mb", function()
				return Stats:GetTotalMemoryUsageMb()
			end)
			read("instances", function()
				return Stats.InstanceCount
			end)
			read("send_kbps", function()
				return Stats.DataSendKbps
			end)
			read("receive_kbps", function()
				return Stats.DataReceiveKbps
			end)
			read("frame_ms", function()
				return Stats.HeartbeatTime * 1000
			end)
			read("players", function()
				return #Players:GetPlayers()
			end)
			queue({ type = "stats", side = "server", metrics = m })
		end
	end)
end

return M
