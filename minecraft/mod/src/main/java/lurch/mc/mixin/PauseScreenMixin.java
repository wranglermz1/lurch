package lurch.mc.mixin;

import lurch.mc.LurchOptionsScreen;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.screens.PauseScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.network.chat.Component;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** A button in the pause menu's top left corner that opens lurch's options. */
@Mixin(PauseScreen.class)
public abstract class PauseScreenMixin extends Screen {
	protected PauseScreenMixin(Component title) {
		super(title);
	}

	@Inject(method = "init", at = @At("RETURN"))
	private void lurch$options(CallbackInfo ci) {
		if (((PauseScreen) (Object) this).showsPauseMenu()) {
			addRenderableWidget(Button.builder(Component.literal("lurch..."), b -> minecraft.gui.setScreen(new LurchOptionsScreen(this))).bounds(4, 4, 80, 20).build());
		}
	}
}
