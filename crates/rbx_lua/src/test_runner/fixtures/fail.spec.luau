return function()
	describe("failures", function()
		it("a failed expectation", function()
			expect(1 + 1).to.equal(3)
		end)

		it("an error", function()
			local value = nil
			return value.field
		end)

		it("passes", function() end)
	end)
end
