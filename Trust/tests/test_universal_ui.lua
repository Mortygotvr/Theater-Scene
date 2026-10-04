-- Test Universal Lua-driven Qt6 UI Engine
local ui = require("theater_ui")
print("✅ Loaded theater_ui.lua module successfully.")

local dock = ui.create_dock("test_dock", "🧪 Test Universal Dock")
if dock then
    print("✅ Created native dock widget handle:", dock.handle)
    local tab1 = dock:add_tab("General", "⚙️")
    print("✅ Created tab 1 handle:", tab1.handle)

    local card1 = tab1:add_card("SETTINGS & SPEED")
    print("✅ Created card handle:", card1.handle)

    local slider = card1:add_slider("Throw Speed", 10, 50, 25, function(val)
        print("[Lua Callback] Slider updated to:", val)
    end, { suffix = " m/s" })
    print("✅ Created slider widget handle:", slider.handle)
    slider:set_value(30)

    local btn = card1:add_button("🚀 Launch Action", function()
        print("[Lua Callback] Button clicked!")
    end, "primary")
    print("✅ Created button widget handle:", btn.handle)

    local txt = card1:add_text_input("Streamer Name", "Morty", function(new_name)
        print("[Lua Callback] Name changed to:", new_name)
    end)
    print("✅ Created text input handle:", txt.handle)
    txt:set_text("MortyGotVR")

    local chk = card1:add_checkbox("Enable Auto-Bounce", true, function(checked)
        print("[Lua Callback] Checkbox toggled:", checked)
    end)
    print("✅ Created checkbox handle:", chk.handle)
    chk:set_checked(false)

    dock:show()

    -- Test Window Creation
    local win = ui.create_window("test_win", "🧪 Test Universal Window", 800, 600)
    assert(win ~= nil, "Window creation failed")
    print("✅ Created native window widget handle:", win.handle)
    local tabWin = win:add_tab("Scene Studio", "🎬")
    local cardWin = tabWin:add_card("ACTIVE SCENE")
    assert(cardWin ~= nil, "Card creation on window failed")
    win:show()

    ui.cleanup()
    print("🎉 Universal UI Engine Unit Test Passed!")
else
    print("❌ Failed to create dock.")
    os.exit(1)
end
