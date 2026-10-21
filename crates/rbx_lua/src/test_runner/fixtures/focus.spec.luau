return function()
	describe("focus", function()
		itFOCUS("focused", function() end)
		it("not focused", function()
			error("must not run")
		end)
	end)
end
