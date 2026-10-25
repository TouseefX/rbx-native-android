-- Mirrors Studio's serializeToString so execute_luau answers read the same.
local MAX_STRING = 10000

local serialize

local function quote(text)
	return '"' .. text:gsub('[%c"\\]', function(c)
		if c == '"' then
			return '\\"'
		elseif c == "\\" then
			return "\\\\"
		elseif c == "\n" then
			return "\\n"
		elseif c == "\r" then
			return "\\r"
		elseif c == "\t" then
			return "\\t"
		end
		return string.format("\\u%04x", string.byte(c))
	end) .. '"'
end

local function cut(text)
	if #text > MAX_STRING then
		return text:sub(1, MAX_STRING) .. "... (truncated)"
	end
	return text
end

-- Studio writes every table as a JSON object, array items keyed "1", "2"...
-- Integer keys come first in order, then string keys sorted.
local function encode(value, path)
	local kind = type(value)
	if kind == "string" then
		return quote(cut(value))
	elseif kind == "number" or kind == "boolean" or kind == "nil" then
		return tostring(value)
	elseif kind ~= "table" then
		return quote(serialize(value))
	end
	if path[value] then
		return nil
	end
	local numbers, strings = {}, {}
	for key in pairs(value) do
		if type(key) == "number" and key % 1 == 0 then
			table.insert(numbers, key)
		elseif type(key) == "string" then
			table.insert(strings, key)
		else
			return nil
		end
	end
	table.sort(numbers)
	table.sort(strings)
	path[value] = true
	local parts = {}
	for _, keys in { numbers, strings } do
		for _, key in keys do
			local text = encode(value[key], path)
			if not text then
				path[value] = nil
				return nil
			end
			table.insert(parts, quote(tostring(key)) .. ":" .. text)
		end
	end
	path[value] = nil
	return "{" .. table.concat(parts, ",") .. "}"
end

function serialize(value)
	local kind = typeof(value)
	if kind == "string" then
		return cut(value)
	elseif kind == "nil" or kind == "boolean" or kind == "number" then
		return tostring(value)
	elseif kind == "Instance" then
		if value.Parent == nil and value ~= game then
			return "[Destroyed Instance]"
		end
		return value:GetFullName()
	elseif kind == "Vector3" then
		return value.X .. ", " .. value.Y .. ", " .. value.Z
	elseif kind == "Vector2" then
		return value.X .. ", " .. value.Y
	elseif kind == "Color3" then
		return value.R .. ", " .. value.G .. ", " .. value.B
	elseif kind == "CFrame" then
		local parts = {}
		for i, component in { value:GetComponents() } do
			parts[i] = tostring(component)
		end
		return table.concat(parts, ", ")
	elseif kind == "table" then
		return encode(value, {}) or "[Complex Table]"
	end
	return tostring(value)
end

return serialize
