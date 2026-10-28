//! Adds a LocalScript to a place that logs the gamepad input it gets, on
//! screen and in the Output: open the result in rbxstudio, open Test ›
//! Controller and press Play to try the Controller Emulator's pad.
//!
//! ```text
//! cargo run -p rbx_play --example gamepad_demo -- <in.rbxl> <out.rbxl>
//! ```

use rbx_dom::{Variant, WeakDom};
use rbx_reflection::ReflectionDatabase;

const DEMO: &str = r#"local UIS = game:GetService("UserInputService")
local CAS = game:GetService("ContextActionService")
local Players = game:GetService("Players")
local gui = Instance.new("ScreenGui")
gui.ResetOnSpawn = false
local label = Instance.new("TextLabel")
label.Size = UDim2.new(0.6, 0, 0, 150)
label.Position = UDim2.new(0.2, 0, 0, 0)
label.BackgroundColor3 = Color3.new(0, 0, 0)
label.BackgroundTransparency = 0.25
label.TextColor3 = Color3.new(1, 1, 1)
label.Font = Enum.Font.Code
label.TextSize = 13
label.TextXAlignment = Enum.TextXAlignment.Left
label.TextYAlignment = Enum.TextYAlignment.Top
label.Parent = gui
gui.Parent = Players.LocalPlayer:WaitForChild("PlayerGui")
local lines = {}
local function log(s)
	print("[paddemo] " .. s)
	table.insert(lines, 1, s)
	if #lines > 10 then table.remove(lines) end
	label.Text = table.concat(lines, "\n")
end
local function pads()
	local names = {}
	for _, pad in UIS:GetConnectedGamepads() do table.insert(names, pad.Name) end
	return table.concat(names, ",")
end
log(string.format("GamepadEnabled=%s connected=%s Last=%s", tostring(UIS.GamepadEnabled), pads(), UIS:GetLastInputType().Name))
UIS.GamepadConnected:Connect(function(pad) log("GamepadConnected " .. pad.Name) end)
UIS.LastInputTypeChanged:Connect(function(t) log("LastInputType " .. t.Name) end)
local function show(kind)
	return function(i, gp)
		if i.UserInputType.Name:find("Gamepad") then
			if i.KeyCode == Enum.KeyCode.Thumbstick1 or i.KeyCode == Enum.KeyCode.ButtonR2 then
				if kind ~= "Changed" or math.abs(i.Position.X) + math.abs(i.Position.Z) > 0.5 then
					log(string.format("%s %s %s %.2f,%.2f,%.2f", kind, i.UserInputType.Name, i.KeyCode.Name, i.Position.X, i.Position.Y, i.Position.Z))
				end
			elseif kind ~= "Changed" then
				log(kind .. " " .. i.UserInputType.Name .. " " .. i.KeyCode.Name .. " gp=" .. tostring(gp))
			end
		end
	end
end
UIS.InputBegan:Connect(show("Began"))
UIS.InputChanged:Connect(show("Changed"))
UIS.InputEnded:Connect(show("Ended"))
CAS:BindAction("PadDemoX", function(_, state, i)
	log("CAS ButtonX " .. state.Name .. " " .. i.KeyCode.Name)
	return Enum.ContextActionResult.Pass
end, false, Enum.KeyCode.ButtonX)
task.spawn(function()
	local was = false
	while true do
		local down = UIS:IsGamepadButtonDown(Enum.UserInputType.Gamepad1, Enum.KeyCode.ButtonY)
		if down ~= was then log("IsGamepadButtonDown ButtonY " .. tostring(down)) was = down end
		task.wait(0.05)
	end
end)
"#;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output] = args.as_slice() else {
        eprintln!("usage: gamepad_demo <in.rbxl> <out.rbxl>");
        std::process::exit(2);
    };
    let bytes = std::fs::read(input).unwrap();
    let mut dom: WeakDom = rbx_binary::deserialize(&bytes).unwrap();
    let root = dom.root_refs()[0];
    let kids = if dom.get(root).unwrap().class() == "DataModel" {
        dom.get(root).unwrap().children().to_vec()
    } else {
        dom.root_refs().to_vec()
    };
    let starter = kids
        .into_iter()
        .find(|r| dom.get(*r).unwrap().class() == "StarterPlayer")
        .unwrap();
    let sps = dom
        .get(starter)
        .unwrap()
        .children()
        .iter()
        .copied()
        .find(|r| dom.get(*r).unwrap().class() == "StarterPlayerScripts")
        .unwrap_or_else(|| {
            dom.new_instance(
                "StarterPlayerScripts",
                "StarterPlayerScripts",
                Some(starter),
            )
        });
    let s = dom.new_instance("LocalScript", "GamepadDemo", Some(sps));
    dom.set_property(s, "Source", Variant::String(DEMO.into()))
        .unwrap();
    let db = ReflectionDatabase::shared();
    let out =
        rbx_binary::serialize_with_defaults(&dom, |c, k| db.stored_default(c, k).cloned()).unwrap();
    std::fs::write(output, out).unwrap();
}
