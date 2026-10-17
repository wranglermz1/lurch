package lurch.mc.mixin;

import com.mojang.renderpearl.api.textures.GpuTextureView;
import lurch.mc.Lurch;
import net.minecraft.client.renderer.GameRenderer;
import net.minecraft.client.renderer.state.level.CameraRenderState;
import net.minecraft.client.renderer.state.level.PlayerRenderState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** No Minecraft hand while lurch's viewmodel is drawn. */
@Mixin(GameRenderer.class)
public class GameRendererMixin {
	@Inject(method = "renderItemInHand", at = @At("HEAD"), cancellable = true)
	private void lurch$hand(CameraRenderState camera, PlayerRenderState player, GpuTextureView target, CallbackInfo ci) {
		if (Lurch.on()) {
			ci.cancel();
		}
	}
}
