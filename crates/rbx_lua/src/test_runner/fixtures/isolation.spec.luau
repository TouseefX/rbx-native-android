return function()
	it("does not see another file's changes", function()
		expect(workspace:GetAttribute("Touched")).toBeNil()
	end)
end
