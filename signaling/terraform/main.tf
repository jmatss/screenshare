provider "aws" {
  region = "eu-north-1"
}

data "aws_region" "current" {}

data "aws_caller_identity" "current" {}

locals {
  environment_variables = {
    TABLE_NAME       = "Screenshare"
    ID_COLUMN_NAME   = "id"
    TYPE_COLUMN_NAME = "type"
  }
  namespace           = "se._2a.screenshare"
  version             = "0.1.0-SNAPSHOT"
  stage_name          = "dev"
  filepath_default    = "${path.module}/../default/target/screenshare-default-${local.version}.jar"
  filepath_connect    = "${path.module}/../connect/target/screenshare-connect-${local.version}.jar"
  filepath_disconnect = "${path.module}/../disconnect/target/screenshare-disconnect-${local.version}.jar"
  filepath_answer     = "${path.module}/../answer/target/screenshare-answer-${local.version}.jar"
  filepath_candidate  = "${path.module}/../candidate/target/screenshare-candidate-${local.version}.jar"
  filepath_offer      = "${path.module}/../offer/target/screenshare-offer-${local.version}.jar"
}

# DynamoDB for storing IDs of connected clients

resource "aws_dynamodb_table" "screenshare-connections" {
  name         = local.environment_variables.TABLE_NAME
  hash_key     = local.environment_variables.ID_COLUMN_NAME
  billing_mode = "PAY_PER_REQUEST"

  on_demand_throughput {
    max_read_request_units  = 10
    max_write_request_units = 10
  }

  attribute {
    name = local.environment_variables.ID_COLUMN_NAME
    type = "S"
  }
}

data "aws_iam_policy_document" "connections-db-policy-document" {
  statement {
    actions = [
      "dynamodb:DeleteItem",
      "dynamodb:PutItem",
      "dynamodb:Scan"
    ]
    effect    = "Allow"
    resources = [aws_dynamodb_table.screenshare-connections.arn]
  }
}

resource "aws_iam_policy" "connections-db-policy" {
  name   = "ScreenshareConnectionsPolicy"
  policy = data.aws_iam_policy_document.connections-db-policy-document.json
}

# API Gateway

resource "aws_apigatewayv2_api" "screenshare-websocket" {
  name                       = "screenshare-websocket"
  protocol_type              = "WEBSOCKET"
  route_selection_expression = "$request.body.action"
}

resource "aws_apigatewayv2_deployment" "deployment" {
  api_id = aws_apigatewayv2_api.screenshare-websocket.id

  depends_on = [
    aws_apigatewayv2_route.connect,
    aws_apigatewayv2_route.default,
    aws_apigatewayv2_route.disconnect,
    aws_apigatewayv2_route.answer,
    aws_apigatewayv2_route.candidate,
    aws_apigatewayv2_route.offer
  ]
}

resource "aws_apigatewayv2_stage" "dev" {
  api_id        = aws_apigatewayv2_api.screenshare-websocket.id
  name          = local.stage_name
  deployment_id = aws_apigatewayv2_deployment.deployment.id

  default_route_settings {
    logging_level          = "INFO"
    throttling_rate_limit  = 100
    throttling_burst_limit = 100
  }
}

data "aws_iam_policy_document" "api-gateway-assume-role-policy-document" {
  statement {
    actions = ["sts:AssumeRole"]
    principals {
      type        = "Service"
      identifiers = ["apigateway.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "api-gateway-role" {
  name               = "ScreenshareApiGatewayRole"
  assume_role_policy = data.aws_iam_policy_document.api-gateway-assume-role-policy-document.json
}

resource "aws_iam_role_policy_attachments_exclusive" "api-gateway-role-policy-attachment" {
  role_name = aws_iam_role.api-gateway-role.name
  policy_arns = [
    "arn:aws:iam::aws:policy/service-role/AmazonAPIGatewayPushToCloudWatchLogs"
  ]
}

# Lambda

data "aws_iam_policy_document" "lambda-assume-role-policy-document" {
  statement {
    actions = ["sts:AssumeRole"]
    principals {
      type        = "Service"
      identifiers = ["lambda.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "lambda-role" {
  name               = "ScreenshareManageConnectionsRole"
  assume_role_policy = data.aws_iam_policy_document.lambda-assume-role-policy-document.json
}

resource "aws_iam_role_policy_attachments_exclusive" "lambda-role-policy-attachment" {
  role_name = aws_iam_role.lambda-role.name
  policy_arns = [
    aws_iam_policy.manage-connections-policy.arn,
    aws_iam_policy.connections-db-policy.arn,
    "arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole"
  ]
}

data "aws_iam_policy_document" "manage-connections-policy-document" {
  statement {
    actions = ["execute-api:ManageConnections"]
    effect  = "Allow"
    resources = [
      aws_apigatewayv2_api.screenshare-websocket.arn,
      "arn:aws:execute-api:${data.aws_region.current.region}:${data.aws_caller_identity.current.account_id}:${aws_apigatewayv2_api.screenshare-websocket.id}/${local.stage_name}/POST/*/*"
    ]
  }
}

resource "aws_iam_policy" "manage-connections-policy" {
  name   = "ScreenshareManageConnectionsPolicy"
  policy = data.aws_iam_policy_document.manage-connections-policy-document.json
}

# API Gateway & Lambda - default

resource "aws_apigatewayv2_integration" "default" {
  api_id             = aws_apigatewayv2_api.screenshare-websocket.id
  integration_type   = "AWS_PROXY"
  integration_method = "POST"
  integration_uri    = aws_lambda_function.default.invoke_arn
}

resource "aws_apigatewayv2_route" "default" {
  api_id    = aws_apigatewayv2_api.screenshare-websocket.id
  target    = "integrations/${aws_apigatewayv2_integration.default.id}"
  route_key = "$default"
}

resource "aws_apigatewayv2_route_response" "default" {
  api_id             = aws_apigatewayv2_api.screenshare-websocket.id
  route_id           = aws_apigatewayv2_route.default.id
  route_response_key = "$default"
}

resource "aws_lambda_function" "default" {
  filename         = local.filepath_default
  function_name    = "default"
  role             = aws_iam_role.lambda-role.arn
  timeout          = 15
  memory_size      = 512
  handler          = "${local.namespace}.DefaultHandler::handleRequest"
  runtime          = "java11"
  source_code_hash = filebase64sha256(local.filepath_default)

  logging_config {
    application_log_level = "INFO"
    log_format            = "JSON"
  }

  environment {
    variables = local.environment_variables
  }
}

resource "aws_lambda_permission" "default" {
  statement_id  = "DefaultInvokeLambda"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.default.function_name
  principal     = "apigateway.amazonaws.com"
  source_arn    = "${aws_apigatewayv2_api.screenshare-websocket.execution_arn}/*"
}

# API Gateway & Lambda - connect

resource "aws_apigatewayv2_integration" "connect" {
  api_id             = aws_apigatewayv2_api.screenshare-websocket.id
  integration_type   = "AWS_PROXY"
  integration_method = "POST"
  integration_uri    = aws_lambda_function.connect.invoke_arn
}

resource "aws_apigatewayv2_route" "connect" {
  api_id    = aws_apigatewayv2_api.screenshare-websocket.id
  target    = "integrations/${aws_apigatewayv2_integration.connect.id}"
  route_key = "$connect"
}

resource "aws_apigatewayv2_route_response" "connect" {
  api_id             = aws_apigatewayv2_api.screenshare-websocket.id
  route_id           = aws_apigatewayv2_route.connect.id
  route_response_key = "$default"
}

resource "aws_lambda_function" "connect" {
  filename         = local.filepath_connect
  function_name    = "connect"
  role             = aws_iam_role.lambda-role.arn
  timeout          = 15
  memory_size      = 512
  handler          = "${local.namespace}.ConnectHandler::handleRequest"
  runtime          = "java11"
  source_code_hash = filebase64sha256(local.filepath_connect)

  logging_config {
    application_log_level = "INFO"
    log_format            = "JSON"
  }

  environment {
    variables = local.environment_variables
  }
}

resource "aws_lambda_permission" "connect" {
  statement_id  = "ConnectInvokeLambda"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.connect.function_name
  principal     = "apigateway.amazonaws.com"
  source_arn    = "${aws_apigatewayv2_api.screenshare-websocket.execution_arn}/*"
}

# API Gateway & Lambda - disconnect

resource "aws_apigatewayv2_integration" "disconnect" {
  api_id             = aws_apigatewayv2_api.screenshare-websocket.id
  integration_type   = "AWS_PROXY"
  integration_method = "POST"
  integration_uri    = aws_lambda_function.disconnect.invoke_arn
}

resource "aws_apigatewayv2_route" "disconnect" {
  api_id    = aws_apigatewayv2_api.screenshare-websocket.id
  target    = "integrations/${aws_apigatewayv2_integration.disconnect.id}"
  route_key = "$disconnect"
}

resource "aws_apigatewayv2_route_response" "disconnect" {
  api_id             = aws_apigatewayv2_api.screenshare-websocket.id
  route_id           = aws_apigatewayv2_route.disconnect.id
  route_response_key = "$default"
}

resource "aws_lambda_function" "disconnect" {
  filename         = local.filepath_disconnect
  function_name    = "disconnect"
  role             = aws_iam_role.lambda-role.arn
  timeout          = 15
  memory_size      = 512
  handler          = "${local.namespace}.DisconnectHandler::handleRequest"
  runtime          = "java11"
  source_code_hash = filebase64sha256(local.filepath_disconnect)

  logging_config {
    application_log_level = "INFO"
    log_format            = "JSON"
  }

  environment {
    variables = local.environment_variables
  }
}

resource "aws_lambda_permission" "disconnect" {
  statement_id  = "DisconnectInvokeLambda"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.disconnect.function_name
  principal     = "apigateway.amazonaws.com"
  source_arn    = "${aws_apigatewayv2_api.screenshare-websocket.execution_arn}/*"
}

# API Gateway & Lambda - answer

resource "aws_apigatewayv2_integration" "answer" {
  api_id             = aws_apigatewayv2_api.screenshare-websocket.id
  integration_type   = "AWS_PROXY"
  integration_method = "POST"
  integration_uri    = aws_lambda_function.answer.invoke_arn
}

resource "aws_apigatewayv2_route" "answer" {
  api_id    = aws_apigatewayv2_api.screenshare-websocket.id
  target    = "integrations/${aws_apigatewayv2_integration.answer.id}"
  route_key = "answer"
}

resource "aws_apigatewayv2_route_response" "answer" {
  api_id             = aws_apigatewayv2_api.screenshare-websocket.id
  route_id           = aws_apigatewayv2_route.answer.id
  route_response_key = "$default"
}

resource "aws_lambda_function" "answer" {
  filename         = local.filepath_answer
  function_name    = "answer"
  role             = aws_iam_role.lambda-role.arn
  timeout          = 15
  memory_size      = 512
  handler          = "${local.namespace}.AnswerHandler::handleRequest"
  runtime          = "java11"
  source_code_hash = filebase64sha256(local.filepath_answer)

  logging_config {
    application_log_level = "INFO"
    log_format            = "JSON"
  }

  environment {
    variables = local.environment_variables
  }
}

resource "aws_lambda_permission" "answer" {
  statement_id  = "AnswerInvokeLambda"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.answer.function_name
  principal     = "apigateway.amazonaws.com"
  source_arn    = "${aws_apigatewayv2_api.screenshare-websocket.execution_arn}/*"
}

# API Gateway & Lambda - candidate

resource "aws_apigatewayv2_integration" "candidate" {
  api_id             = aws_apigatewayv2_api.screenshare-websocket.id
  integration_type   = "AWS_PROXY"
  integration_method = "POST"
  integration_uri    = aws_lambda_function.candidate.invoke_arn
}

resource "aws_apigatewayv2_route" "candidate" {
  api_id    = aws_apigatewayv2_api.screenshare-websocket.id
  target    = "integrations/${aws_apigatewayv2_integration.candidate.id}"
  route_key = "candidate"
}

resource "aws_apigatewayv2_route_response" "candidate" {
  api_id             = aws_apigatewayv2_api.screenshare-websocket.id
  route_id           = aws_apigatewayv2_route.candidate.id
  route_response_key = "$default"
}

resource "aws_lambda_function" "candidate" {
  filename         = local.filepath_candidate
  function_name    = "candidate"
  role             = aws_iam_role.lambda-role.arn
  timeout          = 15
  memory_size      = 512
  handler          = "${local.namespace}.CandidateHandler::handleRequest"
  runtime          = "java11"
  source_code_hash = filebase64sha256(local.filepath_candidate)

  logging_config {
    application_log_level = "INFO"
    log_format            = "JSON"
  }

  environment {
    variables = local.environment_variables
  }
}

resource "aws_lambda_permission" "candidate" {
  statement_id  = "CandidateInvokeLambda"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.candidate.function_name
  principal     = "apigateway.amazonaws.com"
  source_arn    = "${aws_apigatewayv2_api.screenshare-websocket.execution_arn}/*"
}

# API Gateway & Lambda - offer

resource "aws_apigatewayv2_integration" "offer" {
  api_id             = aws_apigatewayv2_api.screenshare-websocket.id
  integration_type   = "AWS_PROXY"
  integration_method = "POST"
  integration_uri    = aws_lambda_function.offer.invoke_arn
}

resource "aws_apigatewayv2_route" "offer" {
  api_id    = aws_apigatewayv2_api.screenshare-websocket.id
  target    = "integrations/${aws_apigatewayv2_integration.offer.id}"
  route_key = "offer"
}

resource "aws_apigatewayv2_route_response" "offer" {
  api_id             = aws_apigatewayv2_api.screenshare-websocket.id
  route_id           = aws_apigatewayv2_route.offer.id
  route_response_key = "$default"
}

resource "aws_lambda_function" "offer" {
  filename         = local.filepath_offer
  function_name    = "offer"
  role             = aws_iam_role.lambda-role.arn
  timeout          = 15
  memory_size      = 512
  handler          = "${local.namespace}.OfferHandler::handleRequest"
  runtime          = "java11"
  source_code_hash = filebase64sha256(local.filepath_offer)

  logging_config {
    application_log_level = "INFO"
    log_format            = "JSON"
  }

  environment {
    variables = local.environment_variables
  }
}

resource "aws_lambda_permission" "offer" {
  statement_id  = "OfferInvokeLambda"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.offer.function_name
  principal     = "apigateway.amazonaws.com"
  source_arn    = "${aws_apigatewayv2_api.screenshare-websocket.execution_arn}/*"
}
