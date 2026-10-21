//! One handler per case: what counts as validating a number, taint
//! followed into the script's own functions, and Lua table fields that
//! aren't Instance properties.

use rbx_audit::{audit, slim_copy, Check};
use rbx_dom::{Variant, WeakDom};

fn place(handler: &str) -> WeakDom {
    let mut dom = WeakDom::new();
    let storage = dom.new_instance("ReplicatedStorage", "ReplicatedStorage", None);
    dom.new_instance("RemoteEvent", "Pay", Some(storage));
    let sss = dom.new_instance("ServerScriptService", "ServerScriptService", None);
    let script = dom.new_instance("Script", "Shop", Some(sss));
    let source = format!(
        "local Pay = game:GetService(\"ReplicatedStorage\").Pay\n\
         local function coins(player) return player.leaderstats.Coins end\n\
         {handler}"
    );
    dom.set_property(script, "Source", Variant::String(source))
        .unwrap();
    dom
}

/// The lines (in `handler`, 1-based) flagged as unvalidated.
fn flagged(handler: &str) -> Vec<usize> {
    audit(&place(handler))
        .iter()
        .filter(|f| f.check == Check::UnvalidatedArgument)
        .map(|f| f.line.unwrap() - 2)
        .collect()
}

#[test]
fn one_bound_does_not_validate_a_number() {
    let handler = "Pay.OnServerEvent:Connect(function(player, amount)
    if amount > 0 then
        coins(player).Value -= amount
    end
end)";
    assert_eq!(flagged(handler), [3]);
}

#[test]
fn two_bounds_do_even_through_a_negated_exit() {
    let both = "Pay.OnServerEvent:Connect(function(player, amount)
    if amount > 0 and amount <= coins(player).Value then
        coins(player).Value -= amount
    end
end)";
    assert!(flagged(both).is_empty());
    let exit = "Pay.OnServerEvent:Connect(function(player, amount)
    if amount < 1 or amount > 100 then
        return
    end
    coins(player).Value -= amount
end)";
    assert!(flagged(exit).is_empty());
    let one_exit = "Pay.OnServerEvent:Connect(function(player, amount)
    if amount < 1 then
        return
    end
    coins(player).Value -= amount
end)";
    assert_eq!(flagged(one_exit), [5]);
}

#[test]
fn a_type_or_nil_check_on_a_number_is_not_a_bound() {
    let typed = "Pay.OnServerEvent:Connect(function(player, amount)
    if typeof(amount) ~= \"number\" then
        return
    end
    coins(player).Value -= amount
end)";
    assert_eq!(flagged(typed), [5]);
    let nil_checked = "Pay.OnServerEvent:Connect(function(player, raw)
    local amount = tonumber(raw)
    if not amount then
        return
    end
    coins(player).Value -= amount
end)";
    assert_eq!(flagged(nil_checked), [6]);
    let clamped = "Pay.OnServerEvent:Connect(function(player, raw)
    coins(player).Value -= math.clamp(tonumber(raw) or 0, 0, 10)
end)";
    assert!(flagged(clamped).is_empty());
    let string_checked = "Pay.OnServerEvent:Connect(function(player, title)
    if typeof(title) ~= \"string\" or #title > 20 then
        return
    end
    player.leaderstats.Title.Value = title
end)";
    assert!(flagged(string_checked).is_empty());
}

#[test]
fn taint_follows_calls_into_the_scripts_functions() {
    let local = "local function give(who, n)
    coins(who).Value += n
end
local Shop = {}
function Shop:refund(who, n)
    coins(who).Value += n
end
Pay.OnServerEvent:Connect(function(player, amount, safe)
    give(player, amount)
    Shop:refund(player, amount)
    give(player, math.clamp(safe, 0, 5))
end)";
    assert_eq!(flagged(local), [2, 6]);
    // The callee's Player parameter is still the sender.
    let confused = "local function give(who, n)
    if who == 5 then return end
end
Pay.OnServerEvent:Connect(function(player, amount)
    give(player, amount)
end)";
    let findings = audit(&place(confused));
    assert!(findings
        .iter()
        .any(|f| f.check == Check::PlayerArgument && f.line == Some(4)));
}

#[test]
fn a_lua_table_field_is_not_an_instance_property() {
    let handler = "Pay.OnServerEvent:Connect(function(player, ...)
    local args = table.pack(...)
    args.n = #args
    local part = args[1]
    part.Anchored = false
end)";
    assert_eq!(flagged(handler), [5]);
}

#[test]
fn the_slim_copy_audits_the_same() {
    let handler = "Pay.OnServerEvent:Connect(function(player, amount)
    coins(player).Value += amount
end)";
    let dom = place(handler);
    let copy = slim_copy(&dom);
    assert_eq!(audit(&copy), audit(&dom));
    assert!(!audit(&dom).is_empty());
}
