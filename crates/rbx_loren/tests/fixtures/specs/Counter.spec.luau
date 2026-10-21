local ReplicatedStorage = game:GetService("ReplicatedStorage")
local Shared = require(ReplicatedStorage:WaitForChild("Shared"))

return function(t)
	t.describe("Counter", function()
		local count = 0
		t.beforeEach(function()
			count = 0
		end)
		t.it("adds", function()
			count = Shared.add(count, 2)
			print("count is", count)
			t.expect(count).toBe(2)
		end)
		t.it("waits a beat", function()
			task.wait(1)
			t.expect(count).toBe(1)
		end)
		t.it("later", function() end, { skip = true })
	end)
end
