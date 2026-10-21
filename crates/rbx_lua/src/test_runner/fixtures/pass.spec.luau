local Shared = require(script.Parent.Shared)

return function()
	describe("TestEZ matchers", function()
		local count = 0
		beforeEach(function()
			count += 1
		end)

		it("equal, ok, never, a", function()
			expect(Shared.add(1, 2)).to.equal(3)
			expect(count).to.be.ok()
			expect(Shared.add(1, 1)).never.to.equal(3)
			expect("x").to.be.a("string")
			expect(0.1 + 0.2).to.be.near(0.3)
		end)

		it("throw", function()
			expect(function()
				error("boom here")
			end).to.throw("boom")
			expect(function() end).never.to.throw()
		end)

		it("waits on the simulated clock", function()
			local before = os.clock()
			task.wait(30)
			expect(count).to.equal(3)
			expect(os.clock() - before < 5).to.equal(true)
		end)
	end)

	describe("Jest-Lua matchers", function()
		it("toBe, toEqual, toContain", function()
			expect(Shared.add(2, 2)).toBe(4)
			expect({ a = { 1, 2 } }).toEqual({ a = { 1, 2 } })
			expect({ 1, 2, 3 }).toContain(2)
			expect("hello").toHaveLength(5)
			expect(nil).toBeNil()
			expect(3).toBeGreaterThan(2)
			expect(1).never.toBe(2)
		end)
	end)
end
