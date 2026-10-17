package lurch.mc.mixin;

import lurch.mc.Lurch;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.LevelReader;
import net.minecraft.world.phys.AABB;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * The integrated server takes the moves lurch makes as they are: Apex's traversals pass through
 * the corner of a ledge, which the server's movement checks would pull the player back from.
 * Only the singleplayer world's own player, and only while lurch drives it.
 */
@Mixin(ServerGamePacketListenerImpl.class)
public class ServerMovementMixin {
	@Shadow public ServerPlayer player;

	@Inject(method = "shouldCheckPlayerMovement", at = @At("HEAD"), cancellable = true)
	private void lurch$tooQuickly(boolean elytra, CallbackInfoReturnable<Boolean> cir) {
		if (Lurch.drives(player.getUUID())) {
			cir.setReturnValue(false);
		}
	}

	@Redirect(method = "handlePlayerPositionChange", at = @At(value = "INVOKE", target = "Lnet/minecraft/server/level/ServerPlayer;isCreative()Z"))
	private boolean lurch$wrongly(ServerPlayer p) {
		return p.isCreative() || Lurch.drives(p.getUUID());
	}

	@Inject(method = "isEntityCollidingWithAnythingNew", at = @At("HEAD"), cancellable = true)
	private void lurch$colliding(LevelReader level, Entity entity, AABB box, double x, double y, double z, CallbackInfoReturnable<Boolean> cir) {
		if (Lurch.drives(entity.getUUID())) {
			cir.setReturnValue(false);
		}
	}
}
