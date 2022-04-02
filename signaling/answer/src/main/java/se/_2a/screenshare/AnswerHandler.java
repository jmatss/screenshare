package se._2a.screenshare;

import java.lang.reflect.Type;
import java.util.Map;

import com.google.gson.Gson;
import com.google.gson.reflect.TypeToken;
import com.amazonaws.services.lambda.runtime.Context;
import com.amazonaws.services.lambda.runtime.LambdaLogger;
import com.amazonaws.services.lambda.runtime.RequestHandler;
import com.amazonaws.services.lambda.runtime.events.APIGatewayV2WebSocketEvent;
import com.amazonaws.services.lambda.runtime.events.APIGatewayV2WebSocketEvent.RequestContext;
import com.amazonaws.services.lambda.runtime.events.APIGatewayV2WebSocketResponse;

/**
 * Class used to "proxy" WebRTC Answer messages between two connected websockets.
 */
public class AnswerHandler implements RequestHandler<APIGatewayV2WebSocketEvent, APIGatewayV2WebSocketResponse> {
    public static final String HANDLER_TYPE = Constants.ANSWER;

    private DynamoDbConnection conn = new DynamoDbConnection();

    @Override
    public APIGatewayV2WebSocketResponse handleRequest(APIGatewayV2WebSocketEvent event, Context context) {
        LambdaLogger logger = context.getLogger();
        Gson gson = new Gson();
        Type jsonType = new TypeToken<Map<String, Object>>(){}.getType();

        RequestContext requestContext = event.getRequestContext();
        String connectionId = requestContext.getConnectionId();

        if (this.conn.isConsumer(connectionId)) {
            String msg = "Not allowed to send RTC answer from consumer.";
            return ResponseUtil.createError(msg, HANDLER_TYPE, logger);
        }

        String producerConnectionId = this.conn.getProducerConnectionId();
        if (producerConnectionId == null) {
            String msg = "Unable to get connection ID for producer.";
            return ResponseUtil.createError(msg, HANDLER_TYPE, logger);
        }

        Map<String, Object> bodyObj = gson.fromJson(event.getBody(), jsonType);
        String consumerConnectionId = (String)bodyObj.remove(Constants.ID);
        String body = gson.toJson(bodyObj);

        ResponseUtil.pushToClient(consumerConnectionId, body, requestContext, logger);
        return ResponseUtil.createSuccess("Answer sent", HANDLER_TYPE, logger);
    }
}
