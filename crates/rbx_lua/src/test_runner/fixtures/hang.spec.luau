return function()
	it("waits on an event nothing fires", function()
		Instance.new("BindableEvent").Event:Wait()
	end)

	it("never runs: the file stopped at the test before", function() end)
end
