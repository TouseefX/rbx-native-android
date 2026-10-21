-- Jest-Lua style: declared at the top level, nothing returned.
describe("top level", function()
	test("works", function()
		workspace:SetAttribute("Touched", true)
		expect(workspace:GetAttribute("Touched")).toBe(true)
	end)
end)

return nil
