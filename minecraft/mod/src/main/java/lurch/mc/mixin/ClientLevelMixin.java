package lurch.mc.mixin;

import lurch.mc.Lurch;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.state.BlockState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Every block that changes in the client's world, so lurch-mc gets the new collision the next frame. */
@Mixin(ClientLevel.class)
public class ClientLevelMixin {
	@Inject(method = "sendBlockUpdated", at = @At("HEAD"))
	private void lurch$changed(BlockPos pos, BlockState old, BlockState now, int flags, CallbackInfo ci) {
		Lurch.blockChanged((ClientLevel) (Object) this, pos, old, now);
	}
}
